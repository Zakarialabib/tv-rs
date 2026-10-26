//! The run loop: one pass over the fleet, and the scheduler around it.
//!
//! [`Reconciler::tick`] is a single pass and is what tests drive; [`Reconciler::run`]
//! is the loop around it. Keeping them separate means the interesting behaviour
//! (decide, act, audit, escalate) is testable without sleeping or spawning.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::oneshot;
use tv_rs_core::{Capability, CommandId, DeviceHandle, ObservedField, ObservedState, Telemetry};

use crate::audit::{AuditEntry, AuditLog};
use crate::desired::DesiredStateProvider;
use crate::healer::{HealerAction, HealerLadder};
use crate::reconciler::{decide, Decision};
use crate::remedy::{plan_remedy, RemedyPolicy, RemedyStep};
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
    /// The numbers the remediation ladder needs (the dim target).
    pub remedy: RemedyPolicy,
}

impl Default for ReconcilePolicy {
    fn default() -> Self {
        Self {
            tick: Duration::from_secs(60),
            max_failures: 3,
            telemetry: Some(TelemetryPolicy::default()),
            remedy: RemedyPolicy::default(),
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

    /// Puts a quarantined device back into rotation.
    ///
    /// Quarantine is a stop, not a verdict: whoever fixed the television — or the
    /// network in front of it — needs a way to say "try this one again" without
    /// restarting the agent. Both halves have to move for that to be true: the
    /// reconciler's own list decides whether a device is visited at all, and the
    /// ladder's count decides which step it resumes on. Releasing one without the
    /// other leaves the device held, or resumes it at the attempt that quarantined
    /// it — which is why this is one call and not two.
    ///
    /// Returns whether the device was actually being held, so a UI can say
    /// "released" rather than "released (it was not quarantined)".
    ///
    /// The attempt is audited either way: a technician grepping the log for a device
    /// should see that someone tried, not only when it worked.
    pub async fn release(&mut self, device_id: &str) -> bool {
        let held = self.quarantined.iter().any(|id| id == device_id);
        self.quarantined.retain(|id| id != device_id);

        if let Ok(id) = tv_rs_core::DeviceId::new(device_id) {
            self.ladder.release(&id);
        }

        record(
            self.audit.as_ref(),
            device_id,
            "in_rotation",
            "release_from_quarantine",
            if held { "ok" } else { "skipped" },
            "inferred",
        )
        .await;

        held
    }

    /// Runs one pass over every device and returns what happened.
    pub async fn tick(&mut self) -> Vec<Outcome> {
        let mut outcomes = Vec::with_capacity(self.fleet.len());

        for handle in &self.fleet {
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

            let outcome = reconcile_one(
                self.provider.as_ref(),
                &self.policy,
                self.audit.as_ref(),
                &mut self.ladder,
                handle.as_ref(),
            )
            .await;

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
}

/// Reads and assesses telemetry when both the policy and the device support it.
///
/// A telemetry read failure is deliberately ignored: diagnostics must not turn a
/// healthy screen into a failed reconciliation. The normal power probe still
/// runs and will surface a genuinely unreachable device.
async fn telemetry_assessment(
    policy: &ReconcilePolicy,
    handle: &dyn DeviceHandle,
) -> Option<(Telemetry, TelemetryVerdict)> {
    let telemetry_policy = policy.telemetry.as_ref()?;
    let provider = handle.as_telemetry()?;
    match provider.telemetry().await {
        Ok(telemetry) => {
            let verdict = assess_telemetry(&telemetry, telemetry_policy);
            Some((telemetry, verdict))
        }
        Err(_) => None,
    }
}

/// Applies the ladder's step for this verdict, when the device can take one.
///
/// Returns the step that was attempted, so the caller can record what the agent
/// actually did rather than what it intended. A failed remedy is not a failed
/// reconciliation: the device is deferred or quarantined either way, and the
/// reason stays the telemetry verdict's.
async fn apply_remedy(
    policy: &ReconcilePolicy,
    handle: &dyn DeviceHandle,
    telemetry: &Telemetry,
    verdict: TelemetryVerdict,
) -> Option<(RemedyStep, Result<(), tv_rs_core::TvError>)> {
    let telemetry_policy = policy.telemetry.as_ref()?;
    let step = plan_remedy(
        telemetry,
        verdict,
        handle.capabilities(),
        telemetry_policy,
        &policy.remedy,
    )?;
    let outcome = match step {
        RemedyStep::DimBacklight { to } => match handle.as_brightness() {
            Some(brightness) => brightness.set_brightness(to, CommandId::new()).await,
            None => Err(tv_rs_core::TvError::Unsupported(Capability::Brightness)),
        },
        RemedyStep::RestartPlayback => match handle.as_key() {
            Some(keys) => {
                keys.key_press(tv_rs_core::Key::Play, CommandId::new())
                    .await
            }
            None => Err(tv_rs_core::TvError::Unsupported(Capability::Key)),
        },
        RemedyStep::RebootPanel => match handle.as_reboot() {
            Some(reboot) => reboot.reboot(CommandId::new()).await,
            None => Err(tv_rs_core::TvError::Unsupported(Capability::Reboot)),
        },
    };
    Some((step, outcome))
}

async fn reconcile_one(
    provider: &dyn DesiredStateProvider,
    policy: &ReconcilePolicy,
    audit: Option<&AuditLog>,
    ladder: &mut HealerLadder,
    handle: &dyn DeviceHandle,
) -> Outcome {
    let id = handle.id();
    let key = id.as_str().to_string();
    let descriptor = handle.descriptor();

    let desired = match provider.desired_state(&descriptor).await {
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
            record(audit, &key, "unknown", "provider", "failed", "unknown").await;
            return Outcome {
                device: key,
                action: None,
                result: "failed",
                error: Some(error.to_string()),
            };
        }
    };

    if let Some((telemetry, verdict)) = telemetry_assessment(policy, handle).await {
        match verdict {
            TelemetryVerdict::Healthy => {}
            TelemetryVerdict::Wait(reason) => {
                // Rung 1: a hot panel is dimmed before it is given up on. The
                // device is still deferred either way — the remedy buys it a
                // chance, it does not prove the panel recovered.
                let remedy = apply_remedy(policy, handle, &telemetry, verdict).await;
                let (action, error) = describe_remedy("telemetry_wait", reason, remedy);
                record(
                    audit,
                    &key,
                    &describe(&desired),
                    action,
                    "deferred",
                    "unknown",
                )
                .await;
                return Outcome {
                    device: key,
                    action: None,
                    result: "deferred",
                    error: Some(error),
                };
            }
            TelemetryVerdict::Quarantine(reason) => {
                // Rung 3, and the only one this path takes: a wedged panel often
                // answers after a restart, and a panel that stays critical is
                // quarantined regardless of whether the reboot helped.
                let remedy = apply_remedy(policy, handle, &telemetry, verdict).await;
                let (action, error) = describe_remedy("telemetry_quarantine", reason, remedy);
                record(
                    audit,
                    &key,
                    &describe(&desired),
                    action,
                    "quarantined",
                    "unknown",
                )
                .await;
                return Outcome {
                    device: key,
                    action: None,
                    result: "quarantined",
                    error: Some(error),
                };
            }
        }
    }

    let observed = observe(handle).await;
    let decision = decide(&desired, &observed, handle.capabilities());

    match act(handle, &decision).await {
        Ok(()) => {
            ladder.on_success(&id);
            record(
                audit,
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
            let action = ladder.on_failure(&id, &error);
            let quarantined = action == HealerAction::Quarantine;
            record(
                audit,
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
async fn act(handle: &dyn DeviceHandle, decision: &Decision) -> Result<(), tv_rs_core::TvError> {
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
    audit: Option<&AuditLog>,
    device: &str,
    desired: &str,
    action: &str,
    result: &str,
    confidence: &str,
) {
    if let Some(log) = audit {
        let entry = AuditEntry::now(device, desired, action, result, confidence);
        // A failed audit write must not stop convergence: the log is for the
        // human afterwards, the screens are for now.
        let _ = log.record(&entry).await;
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

/// The audit label for a ladder step: what was *done*, not what was intended.
fn remedy_label(step: RemedyStep) -> &'static str {
    match step {
        RemedyStep::DimBacklight { .. } => "telemetry_dim",
        RemedyStep::RestartPlayback => "telemetry_restart",
        RemedyStep::RebootPanel => "telemetry_reboot",
    }
}

/// The audit label and error text for a telemetry pass, given what the ladder did.
///
/// `base` is used when the device could not take any step — an absent remedy is an
/// honest "nothing this ladder can do", and the audit line should say exactly that
/// rather than claim a rung that never ran.
fn describe_remedy(
    base: &'static str,
    reason: &'static str,
    remedy: Option<(RemedyStep, Result<(), tv_rs_core::TvError>)>,
) -> (&'static str, String) {
    match remedy {
        None => (base, reason.to_string()),
        Some((step, Ok(()))) => (
            remedy_label(step),
            format!("{reason}; applied {}", step.as_str()),
        ),
        Some((step, Err(error))) => (
            remedy_label(step),
            format!("{reason}; {} failed: {error}", step.as_str()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tv_rs_core::capability::CapabilitySet;
    use tv_rs_core::descriptor::{DeviceDescriptor, DeviceId, DeviceOs};
    use tv_rs_core::mock::{Action, Failure, MockBackend};
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
                remedy: RemedyPolicy::default(),
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

    /// Release has to move both halves, and this is the test that says so. The
    /// reconciler's list decides whether a device is visited at all; the ladder's
    /// count decides which step it resumes on. Moving one without the other leaves
    /// the device held, or resumes it at the attempt that quarantined it.
    ///
    /// The device is one that can never launch (it advertises `PowerState` and
    /// nothing else), so every pass fails the same way and quarantine is
    /// deterministic. `max_failures: 2` is what makes the ladder half observable:
    /// after a release, one more failure must *not* quarantine, because the count
    /// restarted rather than resuming at two.
    #[tokio::test]
    async fn releasing_a_device_returns_it_to_rotation_and_restarts_its_ladder() {
        let (provider, dir) = provider_for(r#"{"tv-6":{"power":"on","app":"netflix"}}"#).await;
        let limited = MockBackend::new(descriptor("tv-6"))
            .with_capabilities(CapabilitySet::of(Capability::PowerState));
        let device = Arc::new(limited);

        let audit_dir = unique_dir("tv-rs-release");
        tokio::fs::create_dir_all(&audit_dir).await.unwrap();
        let log = AuditLog::new(audit_dir.join("audit.jsonl"));

        let mut reconciler = Reconciler::with_policy(
            vec![device.clone()],
            provider,
            ReconcilePolicy {
                max_failures: 2,
                telemetry: None,
                ..ReconcilePolicy::default()
            },
        )
        .with_audit(log.clone());

        // First failure: the ladder's first step, not quarantine.
        let outcomes = reconciler.tick().await;
        assert_ne!(outcomes[0].result, "quarantined", "{outcomes:?}");

        // Second: now it is held.
        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "quarantined", "{outcomes:?}");
        assert_eq!(reconciler.quarantined(), ["tv-6"]);

        // A further pass touches nothing at all: that is what quarantine means.
        let actions_before = device.actions().len();
        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "quarantined", "{outcomes:?}");
        assert_eq!(device.actions().len(), actions_before);

        assert!(reconciler.release("tv-6").await, "it was being held");
        assert!(reconciler.quarantined().is_empty());

        // The count restarted: the next failure is the *first* step again, so it is
        // visited and fails without being quarantined.
        let outcomes = reconciler.tick().await;
        assert_ne!(
            outcomes[0].result, "quarantined",
            "the ladder restarted rather than resuming: {outcomes:?}"
        );

        let text = tokio::fs::read_to_string(log.path()).await.unwrap();
        assert!(text.contains("release_from_quarantine"), "{text}");
        assert!(text.contains("\"ok\""), "{text}");

        tokio::fs::remove_dir_all(dir).await.unwrap();
        tokio::fs::remove_dir_all(audit_dir).await.unwrap();
    }

    /// Releasing a device that was not held is audited as skipped, so a technician
    /// reading the log sees the attempt rather than silence.
    #[tokio::test]
    async fn releasing_a_device_that_was_not_held_is_audited_as_skipped() {
        let (provider, dir) = provider_for(r#"{"tv-7":{"power":"on"}}"#).await;
        let device = Arc::new(MockBackend::new(descriptor("tv-7")));

        let audit_dir = unique_dir("tv-rs-release-skipped");
        tokio::fs::create_dir_all(&audit_dir).await.unwrap();
        let log = AuditLog::new(audit_dir.join("audit.jsonl"));

        let mut reconciler = Reconciler::new(vec![device], provider).with_audit(log.clone());
        assert!(!reconciler.release("tv-7").await, "nothing was held");

        let text = tokio::fs::read_to_string(log.path()).await.unwrap();
        assert!(text.contains("release_from_quarantine"), "{text}");
        assert!(text.contains("\"skipped\""), "{text}");

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
    async fn a_hot_panel_is_dimmed_and_deferred() {
        let (provider, dir) = provider_for(r#"{"tv-hot":{"power":"on","app":"netflix"}}"#).await;
        let mock =
            Arc::new(MockBackend::new(descriptor("tv-hot")).with_telemetry(telemetry_at(70)));
        let mut reconciler = Reconciler::new(vec![mock.clone()], provider);

        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "deferred", "{outcomes:?}");
        assert_eq!(
            outcomes[0].error.as_deref(),
            Some("temperature is high; applied dim_backlight")
        );
        // Rung 1 only: the panel is dimmed, never launched and never toggled.
        assert_eq!(mock.actions(), [Action::SetBrightness(20)]);

        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "deferred");
        assert_eq!(
            mock.actions(),
            [Action::SetBrightness(20), Action::SetBrightness(20)],
            "a panel that is still hot is dimmed again, not escalated"
        );

        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    #[tokio::test]
    async fn a_hot_panel_that_cannot_dim_is_deferred_without_a_command() {
        let (provider, dir) = provider_for(r#"{"tv-plain":{"power":"on","app":"netflix"}}"#).await;
        // No brightness, no reboot: the ladder has nothing to offer, and an absent
        // rung must not become a fake one in the log.
        let capabilities = CapabilitySet::empty()
            .with(Capability::PowerState)
            .with(Capability::Telemetry);
        let mock = Arc::new(
            MockBackend::new(descriptor("tv-plain"))
                .with_capabilities(capabilities)
                .with_telemetry(telemetry_at(70)),
        );
        let mut reconciler = Reconciler::new(vec![mock.clone()], provider);

        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "deferred", "{outcomes:?}");
        assert_eq!(outcomes[0].error.as_deref(), Some("temperature is high"));
        assert!(mock.actions().is_empty());

        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    #[tokio::test]
    async fn a_failed_remedy_is_reported_and_still_defers() {
        let (provider, dir) = provider_for(r#"{"tv-stuck":{"power":"on","app":"netflix"}}"#).await;
        let mock =
            Arc::new(MockBackend::new(descriptor("tv-stuck")).with_telemetry(telemetry_at(70)));
        mock.fail_next(Failure::TimeoutBeforeSend);
        let mut reconciler = Reconciler::new(vec![mock.clone()], provider);

        let outcomes = reconciler.tick().await;
        // A remedy that could not be delivered is not a failed reconciliation: the
        // panel is still deferred, and the audit says the dim did not land.
        assert_eq!(outcomes[0].result, "deferred", "{outcomes:?}");
        let error = outcomes[0].error.as_deref().unwrap_or_default();
        assert!(error.contains("dim_backlight failed"), "{error}");
        assert!(mock.actions().is_empty());

        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    #[tokio::test]
    async fn a_critical_panel_is_rebooted_once_and_quarantined() {
        let (provider, dir) =
            provider_for(r#"{"tv-critical":{"power":"on","app":"netflix"}}"#).await;
        let mock =
            Arc::new(MockBackend::new(descriptor("tv-critical")).with_telemetry(telemetry_at(85)));
        let mut reconciler = Reconciler::new(vec![mock.clone()], provider);

        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "quarantined", "{outcomes:?}");
        assert_eq!(
            outcomes[0].error.as_deref(),
            Some("temperature is critical; applied reboot_panel")
        );
        assert_eq!(reconciler.quarantined(), ["tv-critical".to_string()]);
        assert_eq!(
            mock.actions(),
            [Action::Reboot],
            "the last rung runs once, and nothing else is commanded"
        );

        // A quarantined device is skipped from then on: the reboot is a single
        // attempt, not a loop that keeps hammering a dead panel.
        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "quarantined");
        assert_eq!(mock.actions(), [Action::Reboot]);

        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    #[tokio::test]
    async fn a_reboot_that_fails_still_quarantines() {
        let (provider, dir) = provider_for(r#"{"tv-dead":{"power":"on","app":"netflix"}}"#).await;
        let mock =
            Arc::new(MockBackend::new(descriptor("tv-dead")).with_telemetry(telemetry_at(90)));
        mock.fail_next(Failure::TimeoutBeforeSend);
        let mut reconciler = Reconciler::new(vec![mock.clone()], provider);

        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "quarantined", "{outcomes:?}");
        let error = outcomes[0].error.as_deref().unwrap_or_default();
        assert!(error.contains("reboot_panel failed"), "{error}");
        assert_eq!(reconciler.quarantined(), ["tv-dead".to_string()]);
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
    async fn telemetry_remedies_are_audited_with_the_step_that_ran() {
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
        assert_eq!(hot.actions(), [Action::SetBrightness(20)]);
        assert_eq!(critical.actions(), [Action::Reboot]);

        let text = tokio::fs::read_to_string(log.path()).await.unwrap();
        assert_eq!(text.lines().count(), 2, "{text}");
        // The label names the rung that ran, not the verdict that triggered it.
        assert!(text.contains("\"telemetry_dim\""), "{text}");
        assert!(text.contains("\"telemetry_reboot\""), "{text}");
        assert!(text.contains("\"deferred\""), "{text}");
        assert!(text.contains("\"quarantined\""), "{text}");

        tokio::fs::remove_dir_all(dir).await.unwrap();
        tokio::fs::remove_dir_all(audit_dir).await.unwrap();
    }

    #[tokio::test]
    async fn benchmark_tick_performance() {
        let mut fleet: Vec<Arc<dyn DeviceHandle>> = Vec::with_capacity(1000);
        let mut json = String::from("{");
        for i in 0..1000 {
            let id = format!("tv-{i}");
            if i > 0 {
                json.push(',');
            }
            json.push_str(&format!(r#""{id}":{{"power":"on"}}"#));
            fleet.push(Arc::new(MockBackend::new(descriptor(&id))));
        }
        json.push('}');

        let (provider, dir) = provider_for(&json).await;
        let mut reconciler = Reconciler::new(fleet, provider);

        let start = std::time::Instant::now();
        let iterations = 10;
        for _ in 0..iterations {
            let outcomes = reconciler.tick().await;
            assert_eq!(outcomes.len(), 1000);
        }
        let elapsed = start.elapsed();
        println!(
            "Elapsed for {iterations} ticks of 1000 devices: {:?}",
            elapsed
        );

        tokio::fs::remove_dir_all(dir).await.unwrap();
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
                remedy: RemedyPolicy::default(),
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
