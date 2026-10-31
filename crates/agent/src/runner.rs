//! The run loop: one pass over the fleet, and the scheduler around it.
//!
//! [`Reconciler::tick`] is a single pass and is what tests drive; [`Reconciler::run`]
//! is the loop around it. Keeping them separate means the interesting behaviour
//! (decide, act, audit, escalate) is testable without sleeping or spawning.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::stream::{self, StreamExt};
use tokio::sync::{mpsc, oneshot};
use tv_rs_core::{
    Capability, CommandId, DeviceHandle, DeviceId, ImperativeCommand, LeaseMode, MaintenanceLease,
    ObservedField, ObservedState, Telemetry,
};

use crate::audit::{AuditEntry, AuditLog};
use crate::desired::DesiredStateProvider;
use crate::healer::{HealerAction, HealerLadder};
use crate::reconciler::{decide_with_lease, Decision};
use crate::remedy::{plan_remedy_with_lease, RemedyPolicy, RemedyStep};
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
    /// Maximum number of devices to reconcile concurrently in a single tick pass.
    pub concurrency: usize,
}

impl Default for ReconcilePolicy {
    fn default() -> Self {
        Self {
            tick: Duration::from_secs(60),
            max_failures: 3,
            telemetry: Some(TelemetryPolicy::default()),
            remedy: RemedyPolicy::default(),
            concurrency: 32,
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
    ladder: Mutex<HealerLadder>,
    policy: ReconcilePolicy,
    quarantined: HashSet<DeviceId>,
    /// Active control leases, one per device at most.
    leases: Mutex<HashMap<DeviceId, MaintenanceLease>>,
    /// The imperative relay: lease-holder "do this now" traffic.
    relay: Option<mpsc::Receiver<(DeviceId, ImperativeCommand)>>,
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
            ladder: Mutex::new(HealerLadder::new(policy.max_failures)),
            fleet,
            provider,
            audit: None,
            policy,
            quarantined: HashSet::new(),
            leases: Mutex::new(HashMap::new()),
            relay: None,
        }
    }

    /// Records every action to `log`.
    #[must_use]
    pub fn with_audit(mut self, log: AuditLog) -> Self {
        self.audit = Some(log);
        self
    }

    /// Feeds the imperative relay: lease-holder "do this now" traffic.
    ///
    /// Each message pairs the device with an [`ImperativeCommand`]. The run
    /// loop dispatches a message only while an **active**
    /// [`LeaseMode::ObserveAndRelay`] lease names the same device and
    /// holder-class surface ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â a relay without a live lease is refused, because
    /// "do this now" over an unleased set is exactly the fight the lease
    /// vocabulary stops. Dispatch results are audited and surface as relay
    /// outcomes in `run`'s return value.
    #[must_use]
    pub fn with_command_relay(
        mut self,
        receiver: mpsc::Receiver<(DeviceId, ImperativeCommand)>,
    ) -> Self {
        self.relay = Some(receiver);
        self
    }

    /// Acquires the device's control surface for `lease`.
    ///
    /// Returns the *existing active* lease when one is held, leaving state
    /// untouched: the caller decides to override it (release, then acquire
    /// again) or back off. Only a live lease blocks ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â an expired row is swept
    /// rather than returned.
    ///
    /// This is the ingestion point a commissioning server calls; the HTTP and
    /// MQTT transports for it hang off the facade, and the reconciler only
    /// asks the vocabulary question "is there an active lease?".
    #[must_use]
    pub fn acquire_lease(
        &self,
        lease: MaintenanceLease,
        now_unix_ms: u64,
    ) -> Option<MaintenanceLease> {
        let mut leases = self.leases.lock().expect("lease lock");
        leases.retain(|_, held| held.is_active(now_unix_ms));
        if let Some(existing) = leases.get(&lease.device_id) {
            return Some(existing.clone());
        }
        leases.insert(lease.device_id.clone(), lease);
        None
    }

    /// Extends the device's lease to `expires_at_unix_ms`.
    ///
    /// Returns the updated lease, or `None` when no live lease is held for the
    /// device under this holder ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â a renewal is not an acquisition.
    #[must_use]
    pub fn renew_lease(
        &self,
        device_id: &DeviceId,
        holder_id: &str,
        expires_at_unix_ms: u64,
    ) -> Option<MaintenanceLease> {
        let mut leases = self.leases.lock().expect("lease lock");
        let now = now_unix_ms();
        let held = leases.get_mut(device_id)?;
        // A renewal is not an acquisition: a different holder cannot extend a
        // lease it does not hold, and an expired lease cannot be renewed ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â
        // acquire instead.
        if held.holder_id != holder_id {
            return None;
        }
        if !held.is_active(now) {
            leases.remove(device_id);
            return None;
        }
        held.expires_at_unix_ms = expires_at_unix_ms;
        Some(held.clone())
    }

    /// Releases the device's lease, when `holder_id` is the one holding it.
    ///
    /// Returns whether a live lease was actually dropped, so the surface can
    /// say "released" rather than "released (nothing was held)".
    pub fn release_lease(&self, device_id: &DeviceId, holder_id: &str, now_unix_ms: u64) -> bool {
        let mut leases = self.leases.lock().expect("lease lock");
        if let Some(held) = leases.get(device_id) {
            if held.holder_id == holder_id {
                if held.is_active(now_unix_ms) {
                    leases.remove(device_id);
                    return true;
                }
                // Expired already: sweep it and say nothing was held.
                leases.remove(device_id);
                return false;
            }
        }
        false
    }

    /// The active lease for a device at instant `now_unix_ms`, if any.
    fn active_lease_for(&self, device_id: &DeviceId, now_unix_ms: u64) -> Option<MaintenanceLease> {
        let mut leases = self.leases.lock().expect("lease lock");
        let held = leases.get(device_id)?;
        if held.is_active(now_unix_ms) {
            Some(held.clone())
        } else {
            leases.remove(device_id);
            None
        }
    }

    /// Devices currently out of rotation.
    #[must_use]
    pub fn quarantined(&self) -> &HashSet<DeviceId> {
        &self.quarantined
    }

    /// Puts a quarantined device back into rotation.
    ///
    /// Quarantine is a stop, not a verdict: whoever fixed the television ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â or the
    /// network in front of it ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â needs a way to say "try this one again" without
    /// restarting the agent. Both halves have to move for that to be true: the
    /// reconciler's own list decides whether a device is visited at all, and the
    /// ladder's count decides which step it resumes on. Releasing one without the
    /// other leaves the device held, or resumes it at the attempt that quarantined
    /// it ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â which is why this is one call and not two.
    ///
    /// Returns whether the device was actually being held, so a UI can say
    /// "released" rather than "released (it was not quarantined)".
    ///
    /// The attempt is audited either way: a technician grepping the log for a device
    /// should see that someone tried, not only when it worked.
    pub async fn release(&mut self, device_id: &str) -> bool {
        let mut held = false;

        if let Ok(id) = DeviceId::new(device_id) {
            held = self.quarantined.remove(&id);
            self.ladder.lock().unwrap().release(&id);
        }

        self.record(
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
        let concurrency = self.policy.concurrency.max(1);
        let mut outcomes = Vec::with_capacity(self.fleet.len());
        let mut eligible = Vec::with_capacity(self.fleet.len());

        for handle in &self.fleet {
            let id = handle.id();
            if self.quarantined.contains(&id) {
                outcomes.push(Outcome {
                    device: id.as_str().to_string(),
                    action: None,
                    result: "quarantined",
                    error: None,
                });
            } else {
                eligible.push(handle.clone());
            }
        }

        let reconciler = &*self;
        let mut stream = stream::iter(eligible.into_iter().map(|handle| async move {
            (handle.id(), reconciler.reconcile_one(handle.as_ref()).await)
        }))
        .buffer_unordered(concurrency);

        let mut newly_quarantined = Vec::new();
        while let Some((id, outcome)) = stream.next().await {
            if outcome.result == "quarantined" {
                newly_quarantined.push(id);
            }
            outcomes.push(outcome);
        }
        drop(stream);

        self.quarantined.extend(newly_quarantined);

        outcomes
    }

    /// Runs passes until `stop` fires, relaying imperative commands in between.
    ///
    /// Between ticks, a relay message ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â a device id plus the holder's
    /// [`ImperativeCommand`] ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â is dispatched immediately if, and only if, an
    /// active [`LeaseMode::ObserveAndRelay`] lease covers that device. Relay
    /// outcomes are appended to the returned vector: each is an [`Outcome`]
    /// whose `action` is `None` and whose `result` names the relay verdict.
    ///
    /// A closed relay channel (every sender dropped) is retired silently: the
    /// loop does not busy-fire on it.
    pub async fn run(mut self, stop: oneshot::Receiver<()>) -> Vec<Outcome> {
        // One pass immediately: an agent that waits a full tick before doing
        // anything looks broken on first start.
        let mut last = self.tick().await;
        let mut stop = stop;
        // A separate local, so a select arm borrowing the receiver never
        // conflicts with an arm calling `self.tick()` (which borrows all of
        // `self`).
        let mut relay = self.relay.take();

        loop {
            tokio::select! {
                _ = tokio::time::sleep(self.policy.tick) => {
                    last = self.tick().await;
                }
                message = Self::next_relay_command(&mut relay) => {
                    if let Some((device_id, command)) = message {
                        let outcome = self.relay_command(device_id, &command).await;
                        last.push(outcome);
                    }
                }
                _ = &mut stop => return last,
            }
        }
    }

    /// The next relay message, treating a closed channel as "no more".
    ///
    /// Owning the retired channel inside the future ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â not in the select body ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â
    /// is what keeps a closed channel from busy-firing the loop: once
    /// `recv()` answers `None`, the receiver is dropped into `None` and the
    /// pending future takes over for the closed case.
    async fn next_relay_command(
        relay: &mut Option<mpsc::Receiver<(DeviceId, ImperativeCommand)>>,
    ) -> Option<(DeviceId, ImperativeCommand)> {
        while let Some(receiver) = relay.as_mut() {
            match receiver.recv().await {
                Some(message) => return Some(message),
                // Every sender went away and the backlog drained: retire the
                // channel rather than polling an instant-`None` forever.
                None => *relay = None,
            }
        }
        std::future::pending().await
    }

    /// Reads and assesses telemetry when both the policy and the device support it.
    ///
    /// A telemetry read failure is deliberately ignored: diagnostics must not turn a
    /// healthy screen into a failed reconciliation. The normal power probe still
    /// runs and will surface a genuinely unreachable device.
    async fn telemetry_assessment(
        &self,
        handle: &dyn DeviceHandle,
    ) -> Option<(Telemetry, TelemetryVerdict)> {
        let policy = self.policy.telemetry.as_ref()?;
        let provider = handle.as_telemetry()?;
        match provider.telemetry().await {
            Ok(telemetry) => {
                let verdict = assess_telemetry(&telemetry, policy);
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
        &self,
        handle: &dyn DeviceHandle,
        telemetry: &Telemetry,
        verdict: TelemetryVerdict,
        lease: Option<&MaintenanceLease>,
    ) -> Option<(RemedyStep, Result<(), tv_rs_core::TvError>)> {
        let policy = self.policy.telemetry.as_ref()?;
        let step = plan_remedy_with_lease(
            telemetry,
            verdict,
            handle.capabilities(),
            policy,
            &self.policy.remedy,
            lease,
            now_unix_ms(),
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

    async fn reconcile_one(&self, handle: &dyn DeviceHandle) -> Outcome {
        let id = handle.id();
        let key = id.as_str().to_string();
        let descriptor = handle.descriptor();
        let now = now_unix_ms();
        let lease = self.active_lease_for(&id, now);

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

        if let Some((telemetry, verdict)) = self.telemetry_assessment(handle).await {
            match verdict {
                TelemetryVerdict::Healthy => {}
                TelemetryVerdict::Wait(reason) => {
                    // Rung 1: a hot panel is dimmed before it is given up on. The
                    // device is still deferred either way ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â the remedy buys it a
                    // chance, it does not prove the panel recovered.
                    let remedy = self
                        .apply_remedy(handle, &telemetry, verdict, lease.as_ref())
                        .await;
                    let (action, error) = describe_remedy("telemetry_wait", reason, remedy);
                    self.record(&key, &describe(&desired), action, "deferred", "unknown")
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
                    let remedy = self
                        .apply_remedy(handle, &telemetry, verdict, lease.as_ref())
                        .await;
                    let (action, error) = describe_remedy("telemetry_quarantine", reason, remedy);
                    self.record(&key, &describe(&desired), action, "quarantined", "unknown")
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
        let decision = decide_with_lease(
            &desired,
            &observed,
            handle.capabilities(),
            lease.as_ref(),
            now,
        );

        // A deferred-by-lease pass is not a failure and not a wire touch: the
        // result names the reason so a dashboard can say "borrowed", and the
        // ladder still counts the pass as a success (nothing failed).
        if matches!(
            decision,
            Decision::InhibitedByLease { .. } | Decision::YieldTransport
        ) {
            let holder = match &decision {
                Decision::InhibitedByLease { holder_id } => holder_id.as_str(),
                _ => lease
                    .as_ref()
                    .map(|lease| lease.holder_id.as_str())
                    .unwrap_or("unknown"),
            };
            let (result, reason) = match &decision {
                Decision::InhibitedByLease { .. } => (
                    "inhibited",
                    format!("mutation suppressed under observe_and_relay lease held by {holder}"),
                ),
                _ => (
                    "yielded",
                    format!(
                        "reconciliation yielded under disconnect_and_yield lease held by {holder}"
                    ),
                ),
            };
            self.record(
                &key,
                &describe(&desired),
                &format!("{decision:?}"),
                result,
                "inferred",
            )
            .await;
            self.ladder.lock().unwrap().on_success(&id);
            return Outcome {
                device: key,
                action: Some(decision),
                result,
                error: Some(reason),
            };
        }

        match self.act(handle, &decision).await {
            Ok(()) => {
                self.ladder.lock().unwrap().on_success(&id);
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
                let action = self.ladder.lock().unwrap().on_failure(&id, &error);
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
            // A lease holder drives: the agent's own mutations are loudly
            // suppressed and the transport may be theirs alone. Neither state
            // is a failure, and neither is a wire touch.
            Decision::InhibitedByLease { .. } | Decision::YieldTransport => Ok(()),
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

    /// Dispatches one relay message from the imperative channel.
    ///
    /// Authorization **is** the lease: an active [`LeaseMode::ObserveAndRelay`]
    /// lease naming this device is what makes the holder's `do this now`
    /// traffic legitimate. Without it the command is refused ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â unleased
    /// imperative writes are exactly the wire-fight the vocabulary stops.
    async fn relay_command(&self, device_id: DeviceId, command: &ImperativeCommand) -> Outcome {
        let key = device_id.as_str().to_string();
        let action = format!("relay_{}", command.kind());

        if !matches!(
            self.active_lease_for(&device_id, now_unix_ms()).as_ref(),
            Some(lease) if lease.mode == LeaseMode::ObserveAndRelay
        ) {
            let error =
                format!("no active observe_and_relay lease for {key}; imperative traffic refused");
            self.record(&key, "imperative", &action, "refused", "inferred")
                .await;
            return Outcome {
                device: key,
                action: None,
                result: "refused",
                error: Some(error),
            };
        }

        let Some(handle) = self.fleet.iter().find(|handle| handle.id() == device_id) else {
            let error = format!("{key} is not in the fleet");
            self.record(&key, "imperative", &action, "refused", "inferred")
                .await;
            return Outcome {
                device: key,
                action: None,
                result: "refused",
                error: Some(error),
            };
        };

        match command.dispatch(handle.as_ref(), CommandId::new()).await {
            Ok(()) => {
                self.record(&key, "imperative", &action, "ok", "inferred")
                    .await;
                Outcome {
                    device: key,
                    action: None,
                    result: "relayed",
                    error: None,
                }
            }
            Err(error) => {
                self.record(&key, "imperative", &action, "failed", "inferred")
                    .await;
                Outcome {
                    device: key,
                    action: None,
                    result: "failed",
                    error: Some(error.to_string()),
                }
            }
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

/// The fleet clock: Unix milliseconds from the platform wall clock.
///
/// The lease vocabulary keeps no clock of its own, so the reconciler supplies
/// one here ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â expiry is folded against this instant when a lease is read,
/// renewed or swept.
fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
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
/// `base` is used when the device could not take any step ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â an absent remedy is an
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
                concurrency: 32,
            },
        );

        // The device cannot launch, so every pass fails the same way.
        for _ in 0..3 {
            reconciler.tick().await;
        }
        assert!(reconciler
            .quarantined()
            .contains(&DeviceId::new("tv-3").unwrap()));

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
        log.flush().await.unwrap();

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
        assert!(reconciler
            .quarantined()
            .contains(&DeviceId::new("tv-6").unwrap()));

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

        log.flush().await.unwrap();
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
        log.flush().await.unwrap();

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
        assert!(reconciler
            .quarantined()
            .contains(&DeviceId::new("tv-critical").unwrap()));
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
        assert!(reconciler
            .quarantined()
            .contains(&DeviceId::new("tv-dead").unwrap()));
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
        log.flush().await.unwrap();
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
                concurrency: 32,
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

    // ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ Lease-aware runner tests ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬ÃƒÂ¢Ã¢â‚¬ÂÃ¢â€šÂ¬

    /// Helper: a lease that covers `device` for `holder` with the given mode,
    /// expiring one hour from now.
    fn active_lease(device: &str, holder: &str, mode: LeaseMode) -> MaintenanceLease {
        MaintenanceLease {
            holder_id: holder.to_string(),
            device_id: DeviceId::new(device).unwrap(),
            expires_at_unix_ms: now_unix_ms() + 3_600_000,
            mode,
        }
    }

    /// A tick over a sleeping device held by ObserveAndRelay returns "inhibited"
    /// without touching the device.
    #[tokio::test]
    async fn tick_under_observe_and_relay_lease_is_inhibited() {
        let (provider, dir) = provider_for(r#"{"tv-l1":{"power":"on","app":"netflix"}}"#).await;
        let mock = Arc::new(
            MockBackend::new(descriptor("tv-l1")).starting_at(tv_rs_core::PowerState::Standby),
        );
        let mut reconciler = Reconciler::new(vec![mock.clone()], provider);
        assert!(reconciler
            .acquire_lease(
                active_lease("tv-l1", "holder-A", LeaseMode::ObserveAndRelay),
                now_unix_ms()
            )
            .is_none());
        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes.len(), 1);
        assert_eq!(outcomes[0].result, "inhibited", "{outcomes:?}");
        assert!(
            mock.actions().is_empty(),
            "no command must fire under lease: {:?}",
            mock.actions()
        );
        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    /// A tick over a device held by DisconnectAndYield returns "yielded".
    #[tokio::test]
    async fn tick_under_disconnect_and_yield_lease_is_yielded() {
        let (provider, dir) = provider_for(r#"{"tv-l2":{"power":"on","app":"netflix"}}"#).await;
        let mock = Arc::new(MockBackend::new(descriptor("tv-l2")));
        let mut reconciler = Reconciler::new(vec![mock.clone()], provider);
        assert!(reconciler
            .acquire_lease(
                active_lease("tv-l2", "rig-1", LeaseMode::DisconnectAndYield),
                now_unix_ms()
            )
            .is_none());
        let outcomes = reconciler.tick().await;
        assert_eq!(outcomes[0].result, "yielded", "{outcomes:?}");
        assert!(mock.actions().is_empty(), "{:?}", mock.actions());
        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    /// An imperative Key::Home is relayed when an active ObserveAndRelay lease
    /// covers the device; MockBackend records Action::Key for it.
    #[tokio::test]
    async fn relay_is_accepted_under_an_active_observe_and_relay_lease() {
        use tv_rs_core::Key;
        let (provider, dir) = provider_for(r#"{"tv-l3":{"power":"on"}}"#).await;
        let mock = Arc::new(MockBackend::new(descriptor("tv-l3")));
        let (tx, rx) = mpsc::channel(4);
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let reconciler = Reconciler::new(vec![mock.clone()], provider).with_command_relay(rx);
        assert!(reconciler
            .acquire_lease(
                active_lease("tv-l3", "holder-B", LeaseMode::ObserveAndRelay),
                now_unix_ms()
            )
            .is_none());
        tx.send((
            DeviceId::new("tv-l3").unwrap(),
            tv_rs_core::ImperativeCommand::Key { key: Key::Home },
        ))
        .await
        .unwrap();
        drop(tx);
        let local = tokio::task::LocalSet::new();
        let outcomes = local
            .run_until(async move {
                let handle = tokio::task::spawn_local(reconciler.run(stop_rx));
                tokio::time::sleep(Duration::from_millis(60)).await;
                let _ = stop_tx.send(());
                handle.await.unwrap()
            })
            .await;
        assert!(
            outcomes.iter().any(|o| o.result == "relayed"),
            "expected relayed; got: {outcomes:?}"
        );
        assert!(
            mock.actions()
                .iter()
                .any(|a| matches!(a, tv_rs_core::mock::Action::Key(k) if *k == Key::Home)),
            "Action::Key(Home) not recorded; got: {:?}",
            mock.actions()
        );
        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    /// Without an active lease the relay refuses and never touches the device.
    #[tokio::test]
    async fn relay_is_refused_without_an_active_lease() {
        use tv_rs_core::Key;
        let (provider, dir) = provider_for(r#"{"tv-l4":{"power":"on"}}"#).await;
        let mock = Arc::new(MockBackend::new(descriptor("tv-l4")));
        let (tx, rx) = mpsc::channel(4);
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
        let reconciler = Reconciler::new(vec![mock.clone()], provider).with_command_relay(rx);
        // Keep tx alive until after run() has had a chance to drain the channel.
        tx.send((
            DeviceId::new("tv-l4").unwrap(),
            tv_rs_core::ImperativeCommand::Key { key: Key::Back },
        ))
        .await
        .unwrap();
        let local = tokio::task::LocalSet::new();
        let outcomes = local
            .run_until(async move {
                let handle = tokio::task::spawn_local(reconciler.run(stop_rx));
                // Yield so run() can start, process the initial tick, then drain relay.
                tokio::time::sleep(Duration::from_millis(30)).await;
                drop(tx); // close after run() has had a chance to read
                tokio::time::sleep(Duration::from_millis(30)).await;
                let _ = stop_tx.send(());
                handle.await.unwrap()
            })
            .await;
        assert!(
            outcomes.iter().any(|o| o.result == "refused"),
            "expected refused; got: {outcomes:?}"
        );
        assert!(
            mock.actions().is_empty(),
            "no key without lease: {:?}",
            mock.actions()
        );
        tokio::fs::remove_dir_all(dir).await.unwrap();
    }

    /// acquire_lease blocks a different holder; renew_lease rejects a different holder;
    /// release_lease drops only for the correct holder; after release a new holder can acquire.
    #[tokio::test]
    async fn lease_book_acquire_renew_release_contention() {
        let (provider, dir) = provider_for(r#"{"tv-l5":{"power":"on"}}"#).await;
        let mock = Arc::new(MockBackend::new(descriptor("tv-l5")));
        let reconciler = Reconciler::new(vec![mock], provider);
        let now = now_unix_ms();
        let id = DeviceId::new("tv-l5").unwrap();
        let mk = |holder: &str, mode: LeaseMode, extra_ms: u64| MaintenanceLease {
            holder_id: holder.to_string(),
            device_id: id.clone(),
            expires_at_unix_ms: now + extra_ms,
            mode,
        };

        // First acquire succeeds.
        assert!(reconciler
            .acquire_lease(mk("holder-A", LeaseMode::ObserveAndRelay, 60_000), now)
            .is_none());

        // Second holder blocked ÃƒÂ¢Ã¢â€šÂ¬Ã¢â‚¬Â returns the existing lease.
        let blocked =
            reconciler.acquire_lease(mk("holder-B", LeaseMode::DisconnectAndYield, 120_000), now);
        assert!(blocked.is_some(), "second holder must be blocked");
        assert_eq!(blocked.unwrap().holder_id, "holder-A");

        // Wrong holder cannot renew.
        assert!(reconciler
            .renew_lease(&id, "holder-B", now + 90_000)
            .is_none());

        // Real holder can renew.
        let renewed = reconciler.renew_lease(&id, "holder-A", now + 90_000);
        assert!(renewed.is_some());
        assert_eq!(renewed.unwrap().expires_at_unix_ms, now + 90_000);

        // Wrong holder cannot release.
        assert!(!reconciler.release_lease(&id, "holder-B", now));

        // Real holder can release.
        assert!(reconciler.release_lease(&id, "holder-A", now));

        // After release a new holder can acquire.
        assert!(reconciler
            .acquire_lease(mk("holder-C", LeaseMode::ObserveAndRelay, 60_000), now)
            .is_none());

        tokio::fs::remove_dir_all(dir).await.unwrap();
    }
}
