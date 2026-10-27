//! `CommandId` idempotency at the facade — the layer that owns retries.
//!
//! [`CommandId`]'s contract (`tv-rs-core::command`) states the guarantee once:
//! *two deliveries of the same `CommandId` must have the effect of one* — and
//! deliberately keeps deduplication out of the protocol crates, because the
//! window that makes deduplication meaningful (~30 s for an app launch, seconds
//! for a keypress) is a deployment property of whoever retries. For a consumer
//! driving televisions through `TvClient`, that retry-owner is the facade, and
//! this module is where the contract is kept.
//!
//! # Mechanism
//!
//! [`Dedupe::call`] wraps one mutating operation, keyed by **(device,
//! `CommandId`)**. The device key is the host half of `TvClient::address()` —
//! the same value `DeviceHandle::id` derives — so it is stable across the ways
//! a handle can be built (a bare host, a Roku base URL), clones of one handle
//! share records, and two televisions never do.
//!
//! - Only an `Ok` completion is recorded. An error leaves the id absent, so a
//!   retry after a failure reaches the wire — never a swallowed attempt.
//! - A repeat of a recorded pair inside the window answers `Ok(())` **without
//!   polling the operation**. That is the dangerous direction: the caller
//!   timed out, the command landed, and a second delivery would double the
//!   effect.
//!
//! # Why a process-wide registry
//!
//! `TvClient` is `Clone` and its clones share identity by design — same
//! device, same connection pools. A registry hanging off a single handle would
//! miss the retry that arrives on a fresh clone (or on a handle rebuilt from
//! the same discovery record), which is exactly the double-send this exists to
//! stop. One `static` for the process, keyed per device, covers every handle
//! that can reach a given television; `CommandId`s are UUIDv4 values, unique
//! per creation, so unrelated callers cannot collide on one key.
//!
//! # Bounds
//!
//! State cannot grow without bound, on two axes:
//!
//! - **Window** — [`DEFAULT_WINDOW`] (30 s, per the contract's own wording).
//!   Tests inject a millisecond window through [`Dedupe::with_window`] so
//!   expiry is deterministic without waiting out production time.
//! - **Capacity** — [`DEFAULT_CAPACITY`] recorded successes. Every access
//!   purges expired entries first; if that is not enough, the *oldest* record
//!   is evicted, so the commands closest to a live retry keep their
//!   protection. Saturating the capacity needs 34+ distinct successful
//!   commands per second, process-wide, sustained across a whole window —
//!   beyond anything a television takes.
//!
//! # What this does not do
//!
//! This is not a lock around in-flight commands: two deliveries racing *at the
//! same moment* are not coalesced — only a completed success suppresses a
//! successor. Serializing them would mean holding the mutex across an `.await`,
//! which would make every `async_trait` future on the facade non-`Send`. The
//! contract's scenario — a caller times out, then retries after the first
//! delivery completed — is precisely the sequential case covered here.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use tv_rs_core::{CommandId, TvError};

/// The default window, per the `CommandId` contract's own wording: ~30 seconds
/// covers an app launch, and keypress retries (a few seconds) sit well inside.
const DEFAULT_WINDOW: Duration = Duration::from_secs(30);

/// Bound on recorded successes. 1024 within the 30 s window is 34+ distinct
/// commands per second, sustained — far past what a television (or a fleet
/// driven through one process) takes — while keeping worst-case memory to a
/// few hundred kilobytes.
const DEFAULT_CAPACITY: usize = 1024;

/// The process-wide `CommandId` idempotency registry. See the [module docs]
/// for the contract, the window choice, and why this is one `static`.
pub(crate) struct Dedupe {
    window: Duration,
    capacity: usize,
    registry: Mutex<Registry>,
}

/// Successful completions, per device, newest purge and oldest eviction.
#[derive(Default)]
struct Registry {
    /// Device key → (command → instant of its `Ok` completion).
    devices: HashMap<String, HashMap<CommandId, Instant>>,
}

/// The registry every `TvClient` mutation goes through.
///
/// Process-wide on purpose: handles are `Clone` and share one identity, so
/// their records must be shared too — see the [module docs].
pub(crate) fn global() -> &'static Dedupe {
    static REGISTRY: OnceLock<Dedupe> = OnceLock::new();
    REGISTRY.get_or_init(|| Dedupe::with_window(DEFAULT_WINDOW))
}

impl Dedupe {
    /// A registry with an explicit window — tests inject milliseconds so
    /// expiry is deterministic without waiting out the production 30 s.
    pub(crate) fn with_window(window: Duration) -> Self {
        Self {
            window,
            capacity: DEFAULT_CAPACITY,
            registry: Mutex::new(Registry::default()),
        }
    }

    /// Runs `op` unless the same `(device, cmd)` already succeeded inside the
    /// window, in which case that recorded success replays as `Ok(())` without
    /// polling `op`.
    ///
    /// Failures pass straight through and are never recorded, so they stay
    /// retryable. The operation's error crosses unchanged.
    pub(crate) async fn call<Fut>(
        &self,
        device: &str,
        cmd: CommandId,
        op: Fut,
    ) -> Result<(), TvError>
    where
        Fut: Future<Output = Result<(), TvError>>,
    {
        if self.served(device, cmd) {
            return Ok(());
        }
        let result = op.await;
        if result.is_ok() {
            self.record(device, cmd);
        }
        result
    }

    /// True when a success for `(device, cmd)` is still inside the window;
    /// expired records are purged on the way past.
    fn served(&self, device: &str, cmd: CommandId) -> bool {
        let now = Instant::now();
        let mut registry = self.lock();
        registry.purge(now, self.window);
        registry
            .devices
            .get(device)
            .is_some_and(|sent| sent.contains_key(&cmd))
    }

    /// Remembers a successful completion, keeping both bounds.
    fn record(&self, device: &str, cmd: CommandId) {
        let now = Instant::now();
        let mut registry = self.lock();
        registry.purge(now, self.window);
        if registry.record_count() >= self.capacity {
            registry.evict_oldest();
        }
        registry
            .devices
            .entry(device.to_owned())
            .or_default()
            .insert(cmd, now);
    }

    /// Locks the registry, recovering from poisoning rather than panicking: a
    /// thread that died inside `purge` or `evict_oldest` can only have left
    /// records stale, which the next purge cleans up — never unsound, and
    /// library code does not panic for it.
    fn lock(&self) -> MutexGuard<'_, Registry> {
        self.registry.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Total recorded successes across all devices — lets tests assert on the
    /// registry through the same handle production code locks.
    #[cfg(test)]
    pub(crate) fn record_count(&self) -> usize {
        self.lock().record_count()
    }
}

impl Registry {
    /// Drops every record whose window has elapsed, and the device maps left
    /// empty. Runs on both sides of the lock — on lookup and on record — so
    /// expired state cannot accumulate between accesses.
    fn purge(&mut self, now: Instant, window: Duration) {
        self.devices.retain(|_, sent| {
            sent.retain(|_, at| now.saturating_duration_since(*at) < window);
            !sent.is_empty()
        });
    }

    /// Total recorded successes across all devices.
    fn record_count(&self) -> usize {
        self.devices.values().map(|sent| sent.len()).sum()
    }

    /// Drops the earliest record — the one closest to window expiry — and any
    /// device map it was the last entry of. Reached only at capacity, so the
    /// O(n) scan lives on the rare path.
    fn evict_oldest(&mut self) {
        let oldest = self
            .devices
            .values()
            .flat_map(|sent| sent.values().copied())
            .min();
        let Some(oldest) = oldest else {
            return; // Only reachable with a zero capacity: nothing to evict.
        };
        let target = self.devices.iter().find_map(|(device, sent)| {
            sent.iter()
                .find(|(_, at)| **at == oldest)
                .map(|(cmd, _)| (device.clone(), *cmd))
        });
        let Some((device, cmd)) = target else {
            return;
        };
        let mut emptied = false;
        if let Some(sent) = self.devices.get_mut(&device) {
            sent.remove(&cmd);
            emptied = sent.is_empty();
        }
        if emptied {
            self.devices.remove(&device);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tv_rs_core::Capability;

    use super::*;

    /// The fake backend: counts every attempt it is asked to make.
    async fn serves(calls: &AtomicUsize) -> Result<(), TvError> {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    /// The fake backend refusing — after having been asked (and counted).
    async fn refuses(calls: &AtomicUsize) -> Result<(), TvError> {
        calls.fetch_add(1, Ordering::SeqCst);
        Err(TvError::Unsupported(Capability::Key))
    }

    /// The dangerous case, verbatim: the caller timed out, the command landed,
    /// and it delivers the same id again. Both calls must answer `Ok` — the
    /// second without reaching the wire.
    #[tokio::test]
    async fn the_same_id_delivers_once_and_both_calls_answer_ok() {
        let dedupe = Dedupe::with_window(DEFAULT_WINDOW);
        let calls = AtomicUsize::new(0);
        let cmd = CommandId::new();

        let first = dedupe.call("10.0.0.4", cmd, serves(&calls)).await;
        let second = dedupe.call("10.0.0.4", cmd, serves(&calls)).await;

        assert!(first.is_ok(), "{first:?}");
        assert!(second.is_ok(), "{second:?}");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "the replay must not be sent a second time"
        );
    }

    /// Deduplication is per id: distinct ids are distinct commands and each
    /// must reach the wire, even on the same device.
    #[tokio::test]
    async fn distinct_ids_each_reach_the_wire() {
        let dedupe = Dedupe::with_window(DEFAULT_WINDOW);
        let calls = AtomicUsize::new(0);

        dedupe
            .call("10.0.0.4", CommandId::new(), serves(&calls))
            .await
            .expect("the fake serves");
        dedupe
            .call("10.0.0.4", CommandId::new(), serves(&calls))
            .await
            .expect("the fake serves");

        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "different ids are different commands"
        );
    }

    /// Only `Ok` is recorded: a failure keeps the id absent, so the retry
    /// reaches the wire — and the eventual success is what starts the window.
    #[tokio::test]
    async fn a_failure_is_never_recorded_so_the_retry_reaches_the_wire() {
        let dedupe = Dedupe::with_window(DEFAULT_WINDOW);
        let calls = AtomicUsize::new(0);
        let cmd = CommandId::new();

        let first = dedupe.call("10.0.0.1", cmd, refuses(&calls)).await;
        assert!(first.is_err(), "{first:?}");

        dedupe
            .call("10.0.0.1", cmd, serves(&calls))
            .await
            .expect("the retry goes through");

        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "the failed attempt must not have swallowed the retry"
        );

        dedupe
            .call("10.0.0.1", cmd, serves(&calls))
            .await
            .expect("the replay is local");
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "the success that followed the failure now deduplicates"
        );
    }

    /// The window is a window, not a lifetime: once it elapses the id is live
    /// again and a fresh delivery goes out. A millisecond window and a sleep
    /// that never returns early keep this deterministic.
    #[tokio::test]
    async fn a_record_expires_with_the_window_so_the_id_can_send_again() {
        let dedupe = Dedupe::with_window(Duration::from_millis(5));
        let calls = AtomicUsize::new(0);
        let cmd = CommandId::new();

        dedupe
            .call("10.0.0.4", cmd, serves(&calls))
            .await
            .expect("the fake serves");
        std::thread::sleep(Duration::from_millis(50));
        dedupe
            .call("10.0.0.4", cmd, serves(&calls))
            .await
            .expect("the window has passed");

        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "an expired record must not suppress the delivery"
        );
    }

    /// A record belongs to one television: the same id delivered to a
    /// different device key must go out, or one TV's replay would silence
    /// another's command.
    #[tokio::test]
    async fn records_never_cross_devices() {
        let dedupe = Dedupe::with_window(DEFAULT_WINDOW);
        let calls = AtomicUsize::new(0);
        let cmd = CommandId::new();

        dedupe
            .call("10.0.0.4", cmd, serves(&calls))
            .await
            .expect("the fake serves");
        dedupe
            .call("10.0.0.5", cmd, serves(&calls))
            .await
            .expect("the other television is a different command target");

        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "the record must be keyed per device"
        );
    }

    /// Capacity bound: the registry stops at [`DEFAULT_CAPACITY`] entries and
    /// gives way oldest-first, so recent commands — the ones retries actually
    /// target — keep their protection.
    #[tokio::test]
    async fn records_are_capped_and_the_oldest_record_gives_way() {
        let dedupe = Dedupe::with_window(DEFAULT_WINDOW);
        let calls = AtomicUsize::new(0);
        let dev = "10.0.0.9";

        let first = CommandId::new();
        dedupe
            .call(dev, first, serves(&calls))
            .await
            .expect("the fake serves");
        let mut last = first;
        for _ in 1..DEFAULT_CAPACITY {
            last = CommandId::new();
            dedupe
                .call(dev, last, serves(&calls))
                .await
                .expect("the fake serves");
        }
        assert_eq!(
            dedupe.record_count(),
            DEFAULT_CAPACITY,
            "filled exactly to capacity"
        );

        // Overflow: every additional success evicts one record, the oldest.
        for _ in 0..64 {
            last = CommandId::new();
            dedupe
                .call(dev, last, serves(&calls))
                .await
                .expect("the fake serves");
        }

        assert_eq!(
            dedupe.record_count(),
            DEFAULT_CAPACITY,
            "capacity is a hard bound"
        );
        assert!(
            !dedupe.served(dev, first),
            "the oldest record is the one that gives way"
        );
        assert!(
            dedupe.served(dev, last),
            "recent records keep their protection"
        );
    }

    /// Purge-on-access: records that aged out are gone the moment the registry
    /// is next touched, even though nothing triggers a purge in the meantime.
    #[tokio::test]
    async fn expired_records_purge_on_access() {
        let dedupe = Dedupe::with_window(Duration::from_millis(5));
        let calls = AtomicUsize::new(0);

        for _ in 0..3 {
            dedupe
                .call("10.0.0.7", CommandId::new(), serves(&calls))
                .await
                .expect("the fake serves");
        }
        assert_eq!(dedupe.record_count(), 3);

        std::thread::sleep(Duration::from_millis(50));
        dedupe
            .call("10.0.0.7", CommandId::new(), serves(&calls))
            .await
            .expect("the fake serves");

        assert_eq!(
            dedupe.record_count(),
            1,
            "the three expired records were purged; only the new one remains"
        );
    }
}
