//! The run loop: one pass over the fleet, and the scheduler around it.
//!
//! [`Reconciler::tick`] is a single pass and is what tests drive; [`Reconciler::run`]
//! is the loop around it. Keeping them separate means the interesting behaviour
//! (decide, act, audit, escalate) is testable without sleeping or spawning.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::oneshot;
use tv_rs_core::{Capability, DeviceHandle, ObservedField, ObservedState};

use crate::audit::{AuditEntry, AuditLog};
use crate::desired::DesiredStateProvider;
use crate::healer::{HealerAction, HealerLadder};
use crate::reconciler::{decide, Decision};
use crate::telemetry::{assess_telemetry, TelemetryPolicy, TelemetryVerdict};

/// How hard the loop pushes.
#[derive(Debug, Clone)]
pub struct ReconcilePolicy {
    /// Time between passes over the whole fleet.
    pub tick: Duration,
    /// Whether to stop a device after a repeated failure.
    pub max_failures: u32,
    /// Telemetry thresholds, or `None` to skip telemetry checks entirely.
    pub telemetry: Option<TelemetryPolicy>,
}

impl Default for ReconcilePolicy {
    fn default() -> Self {
        Self {
            tick: Duration::from_secs(60),
            max_failures: 3,
            telemetry: Some(TelemetryPolicy::default()),
        }
    }
}

/// What happened to one device in one pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    /// Device the pass targeted.
    pub device: String,
    /// The action that was attempted, or `None` when the provider had no opinion.
    pub action: Option<Decision>,
    /// `ok`, `skipped`, `deferred`, `failed` or `quarantined`.
    pub result: &'static str,
    /// The error, when something failed.
    pub error: Option<String>,
}

/// Drives a fleet towards its desired state.
pub struct Reconciler {
    fleet: Vec<Arc<dyn DeviceHandle>>,
    provider: Arc<dyn DesiredStateProvider>,
    audit: Option<AuditLog>,
    ladder: HealerLadder,
    policy: ReconcilePolicy,
    quarantined: Vec<String>,
}

impl Reconciler {
    /// A reconciler over `fleet`.
    #[must_use]
    pub fn new(fleet: Vec<Arc<dyn DeviceHandle>>, provider: Arc<dyn DesiredStateProvider>) -> Self {
        Self::with_policy(fleet, provider, ReconcilePolicy::default())
    }

    /// A reconciler with an explicit policy.
    #[must_use]
    pub fn with_policy(
        fleet: Vec<Arc<dyn DeviceHandle>>,
        provider: Arc<dyn DesiredStateProvider>,
        policy: ReconcilePolicy,
    ) -> Self {
        Self {
            ladder: HealerLadder::new(policy.max_failures),
            fleet,
            provider,
            audit: None,
            policy,
            quarantined: Vec::new(),
        }
    }

    /// Records every action to `log`.
    #[must_use]
    pub fn with_audit(mut self, log: AuditLog) -> Self {
        self.audit = Some(log);
        self
    }

    /// Devices currently out of rotation.
    #[must_use]
    pub fn quarantined(&self) -> &[String] {
        &self.quarantined
    }

    /// Runs one pass over every device and returns what happened.
    pub async fn tick(&mut self) -> Vec<Outcome> {
        let mut outcomes = Vec::with_capacity(self.fleet.len());

        for handle in self.fleet.clone() {
            let id = handle.id();
            let key = id.as_str().to_string();

            if self.quarantined.contains(&key) {
                outcomes.push(Outcome {
                    device: key,
                    action: None,
                    result: "quarantined",
                    error: None,
                });
                continue;
            }

            let outcome = self.reconcile_one(handle.as_ref()).await;
            if outcome.result == "quarantined" && !self.quarantined.contains(&outcome.device) {
                self.quarantined.push(outcome.device.clone());
            }
            outcomes.push(outcome);
        }

        outcomes
    }

    /// Runs passes until `stop` fires.
    pub async fn run(mut self, stop: oneshot::Receiver<()>) -> Vec<Outcome> {
        // One pass immediately: an agent that waits a full tick before doing
        // anything looks broken on first start.
        let mut last = self.tick().await;
        let mut stop = stop;

        loop {
            tokio::select! {
                _ = tokio::time::sleep(self.policy.tick) => {
                    last = self.tick().await;
                }
                _ = &mut stop => return last,
            }
        }
    }

    /// Reads and assesses telemetry when both the policy and the device support it.
    ///
    /// A telemetry read failure is deliberately ignored: diagnostics must not turn a
    /// healthy screen into a failed reconciliation. The normal power probe still
    /// runs and will surface a genuinely unreachable device.
    async fn telemetry_verdict(&self, handle: &dyn DeviceHandle) -> Option<TelemetryVerdict> {
        let policy = self.policy.telemetry.as_ref()?;
        let provider = handle.as_telemetry()?;
        match provider.telemetry().await {
            Ok(telemetry) => Some(assess_telemetry(&telemetry, policy)),
            Err(_) => None,
        }
    }

    async fn reconcile_one(&mut self, handle: &dyn DeviceHandle) -> Outcome {
        let id = handle.id();
        let key = id.as_str().to_string();
        let descriptor = handle.descriptor();

        let desired = match self.provider.desired_state(&descriptor).await {
            Ok(Some(desired)) => desired,
            Ok(None) => {
                return Outcome {
                    device: key,
                    action: None,
                    result: "skipped",
                    error: None,
                }
            }
            Err(error) => {
                self.record(&key, "unknown", "provider", "failed", "unknown")
                    .await;
                return Outcome {
                    device: key,
                    action: None,
                    result: "failed",
                    error: Some(error.to_string()),
                };
            }
        };

        if let Some(verdict) = self.telemetry_verdict(handle).await {
            match verdict {
                TelemetryVerdict::Healthy => {}
                TelemetryVerdict::Wait(reason) => {
                    self.record(
                        &key,
                        &describe(&desired),
                        "telemetry_wait",
                        "deferred",
                        "unknown",
                    )
                    .await;
                    return Outcome {
                        device: key,
                        action: None,
                        result: "deferred",
                        error: Some(reason.to_string()),
                    };
                }
                TelemetryVerdict::Quarantine(reason) => {
                    self.record(
                        &key,
                        &describe(&desired),
                        "telemetry_quarantine",
                        "quarantined",
                        "unknown",
                    )
                    .await;
                    return Outcome {
                        device: key,
                        action: None,
                        result: "quarantined",
                        error: Some(reason.to_string()),
                    };
                }
            }
        }

        let observed = observe(handle).await;
        let decision = decide(&desired, &observed, handle.capabilities());

        match self.act(handle, &decision).await {
            Ok(()) => {
                self.ladder.on_success(&id);
                self.record(
                    &key,
                    &describe(&desired),
                    &format!("{decision:?}"),
                    "ok",
                    observed.power.confidence.as_str(),
                )
                .await;
                Outcome {
                    device: key,
                    action: Some(decision),
                    result: "ok",
                    error: None,
                }
            }
            Err(error) => {
                let action = self.ladder.on_failure(&id, &error);
                let quarantined = action == HealerAction::Quarantine;
                self.record(
                    &key,
                    &describe(&desired),
                    &format!("{decision:?}"),
                    if quarantined { "quarantined" } else { "failed" },
                    observed.power.confidence.as_str(),
                )
                .await;
                Outcome {
                    device: key,
                    action: Some(decision),
                    result: if quarantined { "quarantined" } else { "failed" },
                    error: Some(error.to_string()),
                }
            }
        }
    }

    /// Performs the decision. `Nothing` and `ProbePower` are not failures: the first
    /// is the goal, the second means the belief was stale, which the next pass fixes.
    async fn act(
        &self,
        handle: &dyn DeviceHandle,
        decision: &Decision,
    ) -> Result<(), tv_rs_core::TvError> {
        match decision {
            Decision::Nothing | Decision::ProbePower => Ok(()),
            Decision::Wake => match handle.as_power() {
                Some(power) => power.wake().await,
                None => Err(tv_rs_core::TvError::Unsupported(Capability::Wake)),
            },
            Decision::Sleep => match handle.as_power() {
                Some(power) => power.sleep().await,
                None => Err(tv_rs_core::TvError::Unsupported(Capability::PowerState)),
            },
            Decision::Launch(app) => match handle.as_app() {
                Some(launcher) => {
                    launcher
                        .launch(app.clone(), tv_rs_core::CommandId::new())
                        .await
                }
                None => Err(tv_rs_core::TvError::Unsupported(Capability::AppLaunch)),
            },
            Decision::Quarantine(reason) => Err(tv_rs_core::TvError::protocol(*reason)),
        }
    }

    async fn record(
        &self,
        device: &str,
        desired: &str,
        action: &str,
        result: &str,
        confidence: &str,
    ) {
        if let Some(log) = &self.audit {
            let entry = AuditEntry::now(device, desired, action, result, confidence);
            // A failed audit write must not stop convergence: the log is for the
            // human afterwards, the screens are for now.
            let _ = log.record(&entry).await;
        }
    }
}

/// Reads what the device actually reports, marking anything unreadable as unknown.
async fn observe(handle: &dyn DeviceHandle) -> ObservedState {
    let power = match handle.as_power() {
        Some(power) => match power.power_state().await {
            Ok(state) => ObservedField::from_read(state),
            // A failed read is not a state: invalidate rather than carry a stale value.
            Err(_) => ObservedField::unknown(),
        },
        None => ObservedField::unknown(),
    };

    ObservedState {
        power,
        ..ObservedState::nothing_observed()
    }
}

fn describe(desired: &crate::desired::DesiredState) -> String {
    match &desired.app {
        Some(app) => format!("{:?}/{app}", desired.power),
        None => format!("{:?}", desired.power),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tv_rs_core::capability::CapabilitySet;
    use tv_rs_core::descriptor::{DeviceDescriptor, DeviceId, DeviceOs};
    use tv_rs_core::mock::{Failure, MockBackend};
    use tv_rs_core::{Capability, Telemetry};

    use super::*;
    use crate::desired::StaticFileProvider;

    fn unique_dir(prefix: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let tick = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir().join(format!("{prefix}-{}-{nanos}-{tick}", std::process::id()))
    }

    fn descriptor(id: &str) -> DeviceDescriptor {
        DeviceDescriptor::new(DeviceId::new(id).unwrap(), DeviceOs::Roku, "10.0.0.9")
    }

    async fn provider_for(desired_json: &str) -> (Arc<StaticFileProvider>, std::path::PathBuf) {
        let dir = unique_dir("tv-rs-runner");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("desired.json");
        tokio::fs::write(&path, desired_json).await.unwrap();
        (Arc::new(StaticFileProvider::new(&path)), dir)
    }

    #[tokio::test]
    async fn a_pass_launches_what_the_provider_asks_for() {
        let (provider, dir) = provider_for(r#"{"tv-1":{"power":"on","app":"netflix"}}"#).await;
        let mock = Arc::new(MockBackend::new(descriptor("tv-1")));
        let mut reconciler = Reconciler::new(vec![mock.clone()], provider);

        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].result, "ok", "{outcomes:?}");
        assert!(mock.actions().iter().any(|action| matches!(
            action,
            tv_rs_core::mock::Action::Launch(app) if app == "netflix"
        )));

        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    #[tokio::test]
    async fn a_device_without_a_desired_state_is_skipped() {
        let (provider, dir) = provider_for(r#"{}"#).await;
        let mock = Arc::new(MockBackend::new(descriptor("tv-2")));
        let mut reconciler = Reconciler::new(vec![mock], provider);

        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "skipped");

        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    #[tokio::test]
    async fn repeated_failures_quarantine_and_stop_further_actions() {
        let (provider, dir) = provider_for(r#"{"tv-3":{"power":"on","app":"netflix"}}"#).await;
        let limited = MockBackend::new(descriptor("tv-3"))
            .with_capabilities(CapabilitySet::of(Capability::PowerState));
        let mock = Arc::new(limited);
        let mut reconciler = Reconciler::with_policy(
            vec![mock.clone()],
            provider,
            ReconcilePolicy {
                tick: Duration::from_millis(1),
                max_failures: 3,
                telemetry: None,
            },
        );

        // The device cannot launch, so every pass fails the same way.
        for _ in 0..3 {
            reconciler.tick().await;
        }
        assert_eq!(reconciler.quarantined(), ["tv-3".to_string()]);

        let actions_before = mock.actions().len();
        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "quarantined");
        assert_eq!(
            mock.actions().len(),
            actions_before,
            "a quarantined device is not touched again"
        );

        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    #[tokio::test]
    async fn a_failed_launch_is_audited_without_stopping_the_pass() {
        let (provider, dir) = provider_for(
            r#"{"tv-4":{"power":"on","app":"netflix"},"tv-5":{"power":"on","app":"netflix"}}"#,
        )
        .await;

        let failing = Arc::new(MockBackend::new(descriptor("tv-4")));
        failing.fail_next(Failure::AuthExpired);
        let healthy = Arc::new(MockBackend::new(descriptor("tv-5")));

        let audit_dir = unique_dir("tv-rs-audit");
        tokio::fs::create_dir_all(&audit_dir).await.unwrap();
        let log = AuditLog::new(audit_dir.join("audit.jsonl"));

        let mut reconciler =
            Reconciler::new(vec![failing, healthy], provider).with_audit(log.clone());
        let outcomes = reconciler.tick().await;

        assert_eq!(outcomes.len(), 2, "one failure does not abort the fleet");
        let text = tokio::fs::read_to_string(log.path()).await.unwrap();
        assert_eq!(text.lines().count(), 2);
        assert!(text.contains("\"failed\""), "{text}");

        tokio::fs::remove_dir_all(dir).await.unwrap();
        tokio::fs::remove_dir_all(audit_dir).await.unwrap();
    }

    fn telemetry_at(celsius: i16) -> Telemetry {
        Telemetry {
            temperature_celsius: ObservedField::from_read(celsius),
            ..Telemetry::nothing_observed()
        }
    }

    #[tokio::test]
    async fn a_hot_panel_is_deferred_without_a_command() {
        let (provider, dir) = provider_for(r#"{"tv-hot":{"power":"on","app":"netflix"}}"#).await;
        let mock =
            Arc::new(MockBackend::new(descriptor("tv-hot")).with_telemetry(telemetry_at(70)));
        let mut reconciler = Reconciler::new(vec![mock.clone()], provider);

        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "deferred", "{outcomes:?}");
        assert_eq!(outcomes[0].error.as_deref(), Some("temperature is high"));
        assert!(
            mock.actions().is_empty(),
            "a hot panel must not receive a launch or a power command"
        );

        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "deferred");
        assert!(mock.actions().is_empty());

        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    #[tokio::test]
    async fn a_critical_panel_is_quarantined_immediately() {
        let (provider, dir) =
            provider_for(r#"{"tv-critical":{"power":"on","app":"netflix"}}"#).await;
        let mock =
            Arc::new(MockBackend::new(descriptor("tv-critical")).with_telemetry(telemetry_at(85)));
        let mut reconciler = Reconciler::new(vec![mock.clone()], provider);

        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "quarantined", "{outcomes:?}");
        assert_eq!(
            outcomes[0].error.as_deref(),
            Some("temperature is critical")
        );
        assert_eq!(reconciler.quarantined(), ["tv-critical".to_string()]);
        assert!(mock.actions().is_empty());

        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "quarantined");
        assert!(mock.actions().is_empty());

        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    #[tokio::test]
    async fn a_telemetry_read_failure_does_not_block_reconciliation() {
        let (provider, dir) = provider_for(r#"{"tv-diag":{"power":"on","app":"netflix"}}"#).await;
        let mock = Arc::new(MockBackend::new(descriptor("tv-diag")));
        mock.fail_next_telemetry(Failure::TimeoutBeforeSend);
        let mut reconciler = Reconciler::new(vec![mock.clone()], provider);

        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "ok", "{outcomes:?}");
        assert!(mock.actions().iter().any(|action| matches!(
            action,
            tv_rs_core::mock::Action::Launch(app) if app == "netflix"
        )));

        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    #[tokio::test]
    async fn telemetry_verdicts_are_audited_without_device_actions() {
        let (provider, dir) = provider_for(
            r#"{"tv-hot":{"power":"on","app":"netflix"},"tv-critical":{"power":"on","app":"netflix"}}"#,
        )
        .await;
        let hot = Arc::new(MockBackend::new(descriptor("tv-hot")).with_telemetry(telemetry_at(70)));
        let critical =
            Arc::new(MockBackend::new(descriptor("tv-critical")).with_telemetry(telemetry_at(85)));

        let audit_dir = unique_dir("tv-rs-telemetry-audit");
        tokio::fs::create_dir_all(&audit_dir).await.unwrap();
        let log = AuditLog::new(audit_dir.join("audit.jsonl"));
        let mut reconciler =
            Reconciler::new(vec![hot.clone(), critical.clone()], provider).with_audit(log.clone());

        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes.len(), 2);
        assert!(hot.actions().is_empty());
        assert!(critical.actions().is_empty());

        let text = tokio::fs::read_to_string(log.path()).await.unwrap();
        assert_eq!(text.lines().count(), 2, "{text}");
        assert!(text.contains("\"telemetry_wait\""), "{text}");
        assert!(text.contains("\"telemetry_quarantine\""), "{text}");
        assert!(text.contains("\"deferred\""), "{text}");
        assert!(text.contains("\"quarantined\""), "{text}");

        tokio::fs::remove_dir_all(dir).await.unwrap();
        tokio::fs::remove_dir_all(audit_dir).await.unwrap();
    }

    #[tokio::test]
    async fn telemetry_can_be_disabled_by_policy() {
        let (provider, dir) = provider_for(r#"{"tv-off":{"power":"on","app":"netflix"}}"#).await;
        let mock =
            Arc::new(MockBackend::new(descriptor("tv-off")).with_telemetry(telemetry_at(90)));
        let mut reconciler = Reconciler::with_policy(
            vec![mock.clone()],
            provider,
            ReconcilePolicy {
                tick: Duration::from_millis(1),
                max_failures: 3,
                telemetry: None,
            },
        );

        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "ok", "{outcomes:?}");
        assert!(mock.actions().iter().any(|action| matches!(
            action,
            tv_rs_core::mock::Action::Launch(app) if app == "netflix"
        )));

        tokio::fs::remove_dir_all(dir).await.unwrap();
    }
}
