//! Tests for GENA subscription scheduling.
//!
//! Subscription-expiry bugs are invisible until a renderer goes quiet hours
//! later, so every branch of the decision is pinned here rather than trusted to
//! inspection.

use std::time::{Duration, SystemTime};

use super::gena::Subscription;
use super::gena_registry::{
    next_tick_delay, plan_action, SubscriptionAction, SubscriptionRegistry, TrackedSubscription,
    MAX_RENEW_FAILURES, MAX_SUBSCRIBE_FAILURES,
};

fn subscription(timeout_secs: u64, age_secs: u64) -> Subscription {
    Subscription {
        sid: "uuid:sid-1".into(),
        callback_url: "http://192.168.1.2:9999/dlna/events".into(),
        event_sub_url: "http://192.168.1.50:8080/AVT/evt".into(),
        timeout: Duration::from_secs(timeout_secs),
        established: SystemTime::now() - Duration::from_secs(age_secs),
    }
}

// ── plan_action ───────────────────────────────────────────────────────────────

#[test]
fn a_healthy_subscription_is_idle() {
    let t = TrackedSubscription::active(subscription(1800, 10));
    assert_eq!(plan_action(&t, SystemTime::now()), SubscriptionAction::Idle);
}

#[test]
fn a_device_with_no_subscription_is_subscribed() {
    let t = TrackedSubscription::pending();
    assert_eq!(
        plan_action(&t, SystemTime::now()),
        SubscriptionAction::Resubscribe
    );
}

#[test]
fn renewal_is_planned_inside_the_margin() {
    // 1800s grant, 360s margin. At 1500s old, 300s remain.
    let t = TrackedSubscription::active(subscription(1800, 1500));
    assert_eq!(
        plan_action(&t, SystemTime::now()),
        SubscriptionAction::Renew
    );
}

#[test]
fn an_expired_subscription_is_resubscribed_not_renewed() {
    // A lapsed SID cannot be renewed — the renderer has dropped it.
    let t = TrackedSubscription::active(subscription(1800, 3600));
    assert_eq!(
        plan_action(&t, SystemTime::now()),
        SubscriptionAction::Resubscribe
    );
}

#[test]
fn repeated_renew_failures_escalate_to_resubscribe() {
    // Retrying a SID the renderer has forgotten can never succeed. Without this the
    // subscription is dead while the loop looks busy.
    let mut t = TrackedSubscription::active(subscription(1800, 1500));
    t.renew_failures = MAX_RENEW_FAILURES;
    assert_eq!(
        plan_action(&t, SystemTime::now()),
        SubscriptionAction::Resubscribe
    );
}

#[test]
fn one_renew_failure_still_tries_renewing() {
    // Don't throw away a valid SID on a single transient failure.
    let mut t = TrackedSubscription::active(subscription(1800, 1500));
    t.renew_failures = 1;
    assert_eq!(
        plan_action(&t, SystemTime::now()),
        SubscriptionAction::Renew
    );
}

#[test]
fn repeated_subscribe_failures_abandon_the_device() {
    // One unreachable TV must not become a permanent stream of failing HTTP calls on
    // every tick.
    let mut t = TrackedSubscription::pending();
    t.subscribe_failures = MAX_SUBSCRIBE_FAILURES;
    assert_eq!(
        plan_action(&t, SystemTime::now()),
        SubscriptionAction::Abandon
    );
}

#[test]
fn abandonment_outranks_every_other_state() {
    // Even with a live-looking subscription, an abandoned device stays abandoned —
    // otherwise the ordering lets it resurrect itself.
    let mut t = TrackedSubscription::active(subscription(1800, 10));
    t.subscribe_failures = MAX_SUBSCRIBE_FAILURES;
    assert_eq!(
        plan_action(&t, SystemTime::now()),
        SubscriptionAction::Abandon
    );
}

#[test]
fn a_clock_jump_backwards_does_not_wedge_the_scheduler() {
    // If `now` precedes `established` (NTP correction, VM resume), duration_since
    // fails. The subscription must read as healthy rather than panicking or
    // resubscribing in a tight loop.
    let sub = Subscription {
        established: SystemTime::now() + Duration::from_secs(600),
        ..subscription(1800, 0)
    };
    let t = TrackedSubscription::active(sub);
    let action = plan_action(&t, SystemTime::now());
    assert!(
        matches!(action, SubscriptionAction::Idle | SubscriptionAction::Renew),
        "unexpected action for a future-dated subscription: {action:?}"
    );
}

// ── next_tick_delay ───────────────────────────────────────────────────────────

#[test]
fn an_empty_registry_has_no_deadline() {
    // Lets the caller idle on a long default instead of spinning.
    let reg = SubscriptionRegistry::new();
    assert!(reg.next_tick_delay(SystemTime::now()).is_none());
    // The free function shares the behaviour.
    assert!(next_tick_delay(&Default::default(), SystemTime::now()).is_none());
}

#[test]
fn a_pending_device_is_due_immediately() {
    let mut reg = SubscriptionRegistry::new();
    reg.track("uuid:dev-1");
    assert_eq!(
        reg.next_tick_delay(SystemTime::now()),
        Some(Duration::from_secs(1))
    );
}

#[test]
fn the_delay_is_capped_so_new_devices_are_picked_up_promptly() {
    // A fresh 30-minute subscription's own deadline is ~24 min away; sleeping that
    // long would ignore a device discovered a second later.
    let mut reg = SubscriptionRegistry::new();
    reg.record_success("uuid:dev-1", subscription(1800, 0));
    let delay = reg.next_tick_delay(SystemTime::now()).unwrap();
    assert!(delay <= Duration::from_secs(60), "delay was {delay:?}");
}

#[test]
fn an_overdue_subscription_yields_the_minimum_delay_not_zero() {
    // Zero would spin the loop.
    let mut reg = SubscriptionRegistry::new();
    reg.record_success("uuid:dev-1", subscription(1800, 3600));
    assert_eq!(
        reg.next_tick_delay(SystemTime::now()),
        Some(Duration::from_secs(1))
    );
}

#[test]
fn the_soonest_deadline_across_devices_wins() {
    let mut reg = SubscriptionRegistry::new();
    reg.record_success("uuid:fresh", subscription(1800, 0));
    reg.record_success("uuid:urgent", subscription(120, 60));
    let delay = reg.next_tick_delay(SystemTime::now()).unwrap();
    // The 120s grant has a 24s margin, so it is due in ~36s — well under the fresh
    // device's window.
    assert!(delay < Duration::from_secs(60), "delay was {delay:?}");
}

// ── Registry bookkeeping ──────────────────────────────────────────────────────

#[test]
fn tracking_is_idempotent_and_never_discards_a_live_subscription() {
    // A rediscovery sweep calls track() for every device it finds. If that reset the
    // entry, every working subscription would be torn down on each sweep.
    let mut reg = SubscriptionRegistry::new();
    reg.record_success("uuid:dev-1", subscription(1800, 10));
    reg.track("uuid:dev-1");

    let tracked = reg.get("uuid:dev-1").expect("still tracked");
    assert!(
        tracked.subscription.is_some(),
        "track() must not clear an existing subscription"
    );
}

#[test]
fn a_successful_renew_clears_the_failure_counters() {
    let mut reg = SubscriptionRegistry::new();
    reg.track("uuid:dev-1");
    reg.record_renew_failure("uuid:dev-1");
    reg.record_renew_failure("uuid:dev-1");
    reg.record_success("uuid:dev-1", subscription(1800, 0));

    let t = reg.get("uuid:dev-1").unwrap();
    assert_eq!(t.renew_failures, 0);
    assert_eq!(t.subscribe_failures, 0);
}

#[test]
fn a_subscribe_failure_clears_the_stale_subscription() {
    // Otherwise plan_action would choose Renew against a SID the renderer has
    // certainly forgotten.
    let mut reg = SubscriptionRegistry::new();
    reg.record_success("uuid:dev-1", subscription(1800, 10));
    reg.record_subscribe_failure("uuid:dev-1");

    let t = reg.get("uuid:dev-1").unwrap();
    assert!(t.subscription.is_none(), "stale SID must be dropped");
    assert_eq!(t.subscribe_failures, 1);
}

#[test]
fn failure_counters_saturate_rather_than_overflowing() {
    let mut reg = SubscriptionRegistry::new();
    reg.track("uuid:dev-1");
    for _ in 0..10 {
        reg.record_renew_failure("uuid:dev-1");
    }
    assert!(reg.get("uuid:dev-1").unwrap().renew_failures >= MAX_RENEW_FAILURES);
}

#[test]
fn recording_against_an_unknown_device_is_a_no_op() {
    // Events can arrive for a device removed a moment earlier; that must not panic
    // or resurrect it.
    let mut reg = SubscriptionRegistry::new();
    reg.record_renew_failure("uuid:ghost");
    reg.record_subscribe_failure("uuid:ghost");
    assert!(reg.is_empty());
}

#[test]
fn a_sid_maps_back_to_its_device() {
    // The NOTIFY path only knows the SID, so this lookup is what lets an event be
    // attributed to a device at all.
    let mut reg = SubscriptionRegistry::new();
    let mut sub = subscription(1800, 0);
    sub.sid = "uuid:unique-sid".into();
    reg.record_success("uuid:dev-42", sub);

    assert_eq!(
        reg.udn_for_sid("uuid:unique-sid").as_deref(),
        Some("uuid:dev-42")
    );
    assert!(reg.udn_for_sid("uuid:not-mine").is_none());
}

#[test]
fn forgetting_a_device_removes_it() {
    let mut reg = SubscriptionRegistry::new();
    reg.record_success("uuid:dev-1", subscription(1800, 0));
    assert!(reg.forget("uuid:dev-1").is_some());
    assert!(reg.is_empty());
    assert!(
        reg.forget("uuid:dev-1").is_none(),
        "second forget is a no-op"
    );
}

#[test]
fn due_omits_healthy_devices() {
    // The caller iterates this list directly; including Idle entries would mean an HTTP
    // call per healthy device per tick.
    let mut reg = SubscriptionRegistry::new();
    reg.record_success("uuid:healthy", subscription(1800, 10));
    reg.record_success("uuid:stale", subscription(1800, 1700));

    let due = reg.due(SystemTime::now());
    assert_eq!(due.len(), 1, "only the stale device is due: {due:?}");
    assert_eq!(due[0].0, "uuid:stale");
}

#[test]
fn active_subscriptions_lists_only_established_ones() {
    let mut reg = SubscriptionRegistry::new();
    reg.record_success("uuid:live", subscription(1800, 0));
    reg.track("uuid:pending");

    let active = reg.active_subscriptions();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].0, "uuid:live");
    assert_eq!(reg.len(), 2, "both are tracked, only one is active");
}
