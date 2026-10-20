//! Subscription registry and renewal scheduling.
//!
//! Holds the live GENA subscriptions keyed by device UDN and decides *what to do
//! next* — establish, renew, re-establish, or drop.
//!
//! The scheduling decision is a pure function ([`plan_action`]) rather than logic
//! buried in a `tokio::select!` loop. That is deliberate: subscription expiry bugs
//! are invisible until a renderer stops sending events hours later, which is
//! exactly the class of defect that cannot be caught by inspection. As a pure
//! function every branch — including the awkward ones like "clock jumped
//! backwards" — is covered by a test that actually runs.
//!
//! This is bookkeeping only: it never performs I/O. The caller drives the network
//! side ([`crate::gena_client::subscribe`] / [`crate::gena_client::renew`]) and
//! reports outcomes back through [`SubscriptionRegistry::record_success`] and the
//! failure recorders.

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use crate::gena::{Subscription, RENEW_MARGIN_FRACTION};

/// What the renewal loop should do for one device on this tick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscriptionAction {
    /// Healthy and not near expiry — do nothing.
    Idle,
    /// Inside the renewal margin: renew in place with the existing SID.
    Renew,
    /// Lapsed, or renewal failed too many times: SUBSCRIBE from scratch.
    Resubscribe,
    /// Give up on this device until it is rediscovered.
    Abandon,
}

/// Consecutive renewal failures after which we stop retrying and re-subscribe.
pub const MAX_RENEW_FAILURES: u32 = 3;

/// Consecutive *subscribe* failures after which the device is abandoned.
///
/// Higher than the renew threshold because a fresh subscribe failing usually
/// means the renderer is off, and a TV being off is normal rather than an error
/// to escalate.
pub const MAX_SUBSCRIBE_FAILURES: u32 = 5;

/// A tracked subscription plus its failure history.
#[derive(Debug, Clone)]
pub struct TrackedSubscription {
    /// The live subscription, or `None` while pending / after a failed subscribe.
    pub subscription: Option<Subscription>,
    /// Consecutive renew failures since the last success.
    pub renew_failures: u32,
    /// Consecutive subscribe failures since the last success.
    pub subscribe_failures: u32,
}

impl TrackedSubscription {
    /// A device we know about but have not yet subscribed to.
    pub fn pending() -> Self {
        Self {
            subscription: None,
            renew_failures: 0,
            subscribe_failures: 0,
        }
    }

    /// A freshly established subscription.
    pub fn active(subscription: Subscription) -> Self {
        Self {
            subscription: Some(subscription),
            renew_failures: 0,
            subscribe_failures: 0,
        }
    }
}

/// Decide the next action for a tracked subscription.
///
/// Ordering of the checks matters and is the point of this function:
///
/// 1. **Abandonment first.** A device that has failed to subscribe repeatedly
///    must not be retried forever on every tick — that turns one unreachable TV
///    into a permanent stream of failing HTTP calls.
/// 2. **No subscription → subscribe** (reported as `Resubscribe`, since
///    establishing and re-establishing are the same call).
/// 3. **Too many renew failures → re-subscribe,** not another renew. Retrying a
///    SID the renderer has forgotten can never succeed, so without this the
///    subscription is dead but the loop looks busy.
/// 4. **Expired → re-subscribe.** A lapsed SID cannot be renewed.
/// 5. **Inside the margin → renew.**
pub fn plan_action(tracked: &TrackedSubscription, now: SystemTime) -> SubscriptionAction {
    if tracked.subscribe_failures >= MAX_SUBSCRIBE_FAILURES {
        return SubscriptionAction::Abandon;
    }

    let Some(sub) = tracked.subscription.as_ref() else {
        return SubscriptionAction::Resubscribe;
    };

    if tracked.renew_failures >= MAX_RENEW_FAILURES {
        return SubscriptionAction::Resubscribe;
    }

    if sub.is_expired(now) {
        return SubscriptionAction::Resubscribe;
    }

    if sub.needs_renewal(now) {
        return SubscriptionAction::Renew;
    }

    SubscriptionAction::Idle
}

/// How long the loop may sleep before the earliest subscription needs attention.
///
/// Returns `None` when nothing is tracked, so the caller can idle on a long
/// default instead of spinning.
///
/// Clamped to `[1s, 60s]`. The lower bound stops a busy-loop when something is
/// already overdue; the upper bound means a newly discovered device is picked up
/// within a minute even though its own deadline is half an hour away.
pub fn next_tick_delay(
    registry: &HashMap<String, TrackedSubscription>,
    now: SystemTime,
) -> Option<Duration> {
    if registry.is_empty() {
        return None;
    }

    let mut soonest: Option<Duration> = None;

    for tracked in registry.values() {
        // Anything not currently healthy needs attention on the next tick.
        let Some(sub) = tracked.subscription.as_ref() else {
            return Some(Duration::from_secs(1));
        };

        let margin = sub.timeout / RENEW_MARGIN_FRACTION;
        let deadline = sub.expires_at() - margin;

        match deadline.duration_since(now) {
            Ok(remaining) => {
                soonest = Some(match soonest {
                    Some(cur) if cur <= remaining => cur,
                    _ => remaining,
                });
            }
            // Already past the renewal point.
            Err(_) => return Some(Duration::from_secs(1)),
        }
    }

    soonest.map(|d| d.clamp(Duration::from_secs(1), Duration::from_secs(60)))
}

/// Registry of subscriptions keyed by device UDN.
#[derive(Debug, Default)]
pub struct SubscriptionRegistry {
    entries: HashMap<String, TrackedSubscription>,
}

impl SubscriptionRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Begin tracking a device, without subscribing yet.
    ///
    /// Idempotent: re-registering a device that already has a live subscription
    /// must not discard it, or a rediscovery sweep would tear down every working
    /// subscription it finds.
    pub fn track(&mut self, udn: &str) {
        self.entries
            .entry(udn.to_string())
            .or_insert_with(TrackedSubscription::pending);
    }

    /// Record a successful subscribe or renew.
    pub fn record_success(&mut self, udn: &str, subscription: Subscription) {
        self.entries
            .insert(udn.to_string(), TrackedSubscription::active(subscription));
    }

    /// Record a failed renew, keeping the existing subscription for one more
    /// attempt.
    pub fn record_renew_failure(&mut self, udn: &str) {
        if let Some(entry) = self.entries.get_mut(udn) {
            entry.renew_failures = entry.renew_failures.saturating_add(1);
        }
    }

    /// Record a failed subscribe and clear any stale subscription.
    ///
    /// The subscription is dropped because a failed *subscribe* means we have no
    /// valid SID; keeping the old one would make [`plan_action`] choose `Renew`
    /// against a SID the renderer has certainly forgotten.
    pub fn record_subscribe_failure(&mut self, udn: &str) {
        if let Some(entry) = self.entries.get_mut(udn) {
            entry.subscribe_failures = entry.subscribe_failures.saturating_add(1);
            entry.subscription = None;
        }
    }

    /// Stop tracking a device entirely.
    ///
    /// Returns the removed entry so the caller can `UNSUBSCRIBE` it; `None` when
    /// the device was not tracked (also a no-op on the second call).
    pub fn forget(&mut self, udn: &str) -> Option<TrackedSubscription> {
        self.entries.remove(udn)
    }

    /// Look up the subscription for a device.
    pub fn get(&self, udn: &str) -> Option<&TrackedSubscription> {
        self.entries.get(udn)
    }

    /// Find the device that owns a SID.
    ///
    /// Needed on the NOTIFY path: the renderer identifies itself by SID, not UDN,
    /// so an incoming event has to be mapped back to a device before it can be
    /// reported.
    pub fn udn_for_sid(&self, sid: &str) -> Option<String> {
        self.entries.iter().find_map(|(udn, tracked)| {
            tracked
                .subscription
                .as_ref()
                .filter(|s| s.sid == sid)
                .map(|_| udn.clone())
        })
    }

    /// Every device needing action this tick, paired with what to do.
    ///
    /// `Idle` entries are omitted so the caller iterates only over real work.
    pub fn due(&self, now: SystemTime) -> Vec<(String, SubscriptionAction)> {
        self.entries
            .iter()
            .filter_map(|(udn, tracked)| match plan_action(tracked, now) {
                SubscriptionAction::Idle => None,
                action => Some((udn.clone(), action)),
            })
            .collect()
    }

    /// Sleep duration until the next deadline.
    pub fn next_tick_delay(&self, now: SystemTime) -> Option<Duration> {
        next_tick_delay(&self.entries, now)
    }

    /// Number of tracked devices.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Are there no tracked devices at all?
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Snapshot of live subscriptions, for diagnostics.
    pub fn active_subscriptions(&self) -> Vec<(String, Subscription)> {
        self.entries
            .iter()
            .filter_map(|(udn, t)| t.subscription.clone().map(|s| (udn.clone(), s)))
            .collect()
    }
}
