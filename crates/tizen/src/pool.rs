//! A per-TV connection pool for the remote channel.
//!
//! Samsung TVs drop idle WebSocket channels — a player that sends a key press
//! once every few hours will find its socket dead on the next call. Reconnecting
//! on every press costs a handshake (and, without a stored token, a PIN prompt),
//! so the pool keeps one channel per TV and reopens it exactly once when a send
//! fails.
//!
//! ```
//! # async fn demo() -> tv_rs_tizen::Result<()> {
//! use std::time::Duration;
//! use tv_rs_tizen::{TizenClient, TizenConnectionPool, TizenRemoteKey};
//!
//! let pool = TizenConnectionPool::new();
//! let client = TizenClient::new("192.168.1.10").with_token("saved-token");
//!
//! pool.send_key(&client, TizenRemoteKey::Power).await?;
//! pool.send_key(&client, TizenRemoteKey::Power).await?; // reuses the channel
//! assert_eq!(pool.len().await, 1);
//! pool.evict("192.168.1.10").await;
//! # Ok(()) }
//! ```
//!
//! The pool serialises sends per TV, which matches the protocol: the remote
//! channel is single-writer.

use std::collections::HashMap;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::client::{TizenClient, TizenSession};
use crate::error::Result;
use crate::types::TizenRemoteKey;

/// A live remote channel that can accept frames.
///
/// Implemented for [`TizenSession`]; the trait exists so the pool can be tested
/// without a television.
#[async_trait]
pub trait TizenChannel: Send {
    /// Sends one remote-key press.
    async fn send_key(&mut self, key: TizenRemoteKey) -> Result<()>;

    /// Sends a raw text frame.
    async fn send_raw(&mut self, raw: &str) -> Result<()>;
}

#[async_trait]
impl TizenChannel for TizenSession {
    async fn send_key(&mut self, key: TizenRemoteKey) -> Result<()> {
        TizenSession::send_key(self, key).await
    }

    async fn send_raw(&mut self, raw: &str) -> Result<()> {
        TizenSession::send_raw(self, raw).await
    }
}

/// Opens channels for the pool.
#[async_trait]
pub trait TizenChannelFactory: Send + Sync {
    /// Opens a fresh channel for `client`, using the token it carries.
    async fn open(&self, client: &TizenClient) -> Result<Box<dyn TizenChannel>>;
}

/// The production factory: a real `ms.channel.connect` handshake.
#[derive(Debug, Clone, Copy, Default)]
pub struct LiveChannelFactory;

#[async_trait]
impl TizenChannelFactory for LiveChannelFactory {
    async fn open(&self, client: &TizenClient) -> Result<Box<dyn TizenChannel>> {
        let session = TizenSession::connect(client).await?;
        Ok(Box::new(session))
    }
}

/// One frame to send through the pool.
enum Op<'a> {
    /// A typed remote-key press.
    Key(TizenRemoteKey),
    /// A raw text frame.
    Raw(&'a str),
}

/// One pooled channel per TV, keyed by host.
pub struct TizenConnectionPool<F: TizenChannelFactory = LiveChannelFactory> {
    factory: F,
    channels: Mutex<HashMap<String, Box<dyn TizenChannel>>>,
}

impl TizenConnectionPool<LiveChannelFactory> {
    /// Creates a pool backed by real handshakes.
    pub fn new() -> Self {
        Self::with_factory(LiveChannelFactory)
    }
}

impl Default for TizenConnectionPool<LiveChannelFactory> {
    fn default() -> Self {
        Self::new()
    }
}

impl<F: TizenChannelFactory> TizenConnectionPool<F> {
    /// Creates a pool with a custom channel factory (tests, instrumentation).
    pub fn with_factory(factory: F) -> Self {
        Self {
            factory,
            channels: Mutex::new(HashMap::new()),
        }
    }

    /// Sends a key press to the TV behind `client`, opening or refreshing the
    /// channel as needed.
    ///
    /// A send that fails against a pooled channel is retried **once** on a fresh
    /// connection: a dead idle socket must not surface to the user as a failed
    /// button press. A second failure is returned.
    pub async fn send_key(&self, client: &TizenClient, key: TizenRemoteKey) -> Result<()> {
        self.dispatch(client, &Op::Key(key)).await
    }

    /// Sends a raw frame to the TV behind `client`, with the same open/retry
    /// behaviour as [`send_key`](Self::send_key).
    pub async fn send_raw(&self, client: &TizenClient, raw: &str) -> Result<()> {
        self.dispatch(client, &Op::Raw(raw)).await
    }

    /// Drops the pooled channel for `host`; `true` when one was connected.
    ///
    /// Call this when a TV is removed from the fleet, or when the caller knows it
    /// is powering off — the next send opens a fresh channel.
    pub async fn evict(&self, host: &str) -> bool {
        self.channels.lock().await.remove(host).is_some()
    }

    /// Number of TV channels currently pooled.
    pub async fn len(&self) -> usize {
        self.channels.lock().await.len()
    }

    /// `true` when nothing is pooled.
    pub async fn is_empty(&self) -> bool {
        self.channels.lock().await.is_empty()
    }

    /// `true` when `host` has a pooled channel.
    pub async fn contains(&self, host: &str) -> bool {
        self.channels.lock().await.contains_key(host)
    }

    async fn dispatch(&self, client: &TizenClient, op: &Op<'_>) -> Result<()> {
        let host = client.host().to_string();

        let mut channels = self.channels.lock().await;
        if !channels.contains_key(&host) {
            let channel = self.factory.open(client).await?;
            channels.insert(host.clone(), channel);
        }

        let channel = channels
            .get_mut(&host)
            .expect("channel was just inserted or already present");

        let first = match op {
            Op::Key(key) => channel.send_key(*key).await,
            Op::Raw(raw) => channel.send_raw(raw).await,
        };
        if first.is_ok() {
            return Ok(());
        }

        // Dead channel (idle timeout, TV switched input): reopen once and retry.
        channels.remove(&host);
        let mut fresh = self.factory.open(client).await?;
        let retry = match op {
            Op::Key(key) => fresh.send_key(*key).await,
            Op::Raw(raw) => fresh.send_raw(raw).await,
        };
        channels.insert(host, fresh);
        retry
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    // `super::*` brings tokio's async Mutex; the fake channel needs a plain one.
    use std::sync::Mutex as StdMutex;

    use super::*;
    use crate::error::TizenError;

    /// Channel that spends from a shared budget of simulated failures.
    ///
    /// A shared budget is what makes the retry observable: the first channel
    /// fails, the reopened one succeeds.
    struct FakeChannel {
        failures: Arc<StdMutex<usize>>,
        sent: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl TizenChannel for FakeChannel {
        async fn send_key(&mut self, _key: TizenRemoteKey) -> Result<()> {
            let mut budget = self.failures.lock().expect("test mutex");
            if *budget > 0 {
                *budget -= 1;
                return Err(TizenError::Protocol("dead channel".into()));
            }
            drop(budget);
            self.sent.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn send_raw(&mut self, _raw: &str) -> Result<()> {
            self.sent.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct FakeFactory {
        opens: Arc<AtomicUsize>,
        failures: Arc<StdMutex<usize>>,
        sent: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl TizenChannelFactory for FakeFactory {
        async fn open(&self, _client: &TizenClient) -> Result<Box<dyn TizenChannel>> {
            self.opens.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeChannel {
                failures: self.failures.clone(),
                sent: self.sent.clone(),
            }))
        }
    }

    /// `(opens, sent)` counters, both starting at zero.
    fn counters() -> (Arc<AtomicUsize>, Arc<AtomicUsize>) {
        (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)))
    }

    fn factory(opens: Arc<AtomicUsize>, failures: usize, sent: Arc<AtomicUsize>) -> FakeFactory {
        FakeFactory {
            opens,
            failures: Arc::new(StdMutex::new(failures)),
            sent,
        }
    }

    #[tokio::test]
    async fn reuses_one_channel_for_repeated_sends() {
        let (opens, sent) = counters();
        let pool = TizenConnectionPool::with_factory(factory(opens.clone(), 0, sent.clone()));
        let client = TizenClient::new("10.0.0.1");

        pool.send_key(&client, TizenRemoteKey::Power).await.unwrap();
        pool.send_key(&client, TizenRemoteKey::Up).await.unwrap();

        assert_eq!(opens.load(Ordering::SeqCst), 1, "channel must be reused");
        assert_eq!(sent.load(Ordering::SeqCst), 2);
        assert_eq!(pool.len().await, 1);
    }

    #[tokio::test]
    async fn a_dead_channel_is_reopened_once_and_the_send_retried() {
        let (opens, sent) = counters();
        let pool = TizenConnectionPool::with_factory(factory(opens.clone(), 1, sent.clone()));
        let client = TizenClient::new("10.0.0.2");

        pool.send_key(&client, TizenRemoteKey::Home).await.unwrap();

        assert_eq!(opens.load(Ordering::SeqCst), 2, "one reopen");
        assert_eq!(
            sent.load(Ordering::SeqCst),
            1,
            "retry delivered exactly once"
        );
    }

    #[tokio::test]
    async fn a_second_failure_is_returned_to_the_caller() {
        let (opens, sent) = counters();
        let pool = TizenConnectionPool::with_factory(factory(opens.clone(), 2, sent.clone()));
        let client = TizenClient::new("10.0.0.3");

        let result = pool.send_key(&client, TizenRemoteKey::Home).await;

        assert!(result.is_err(), "two failures must surface");
        assert_eq!(opens.load(Ordering::SeqCst), 2, "retry is not a loop");
        assert_eq!(sent.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn evict_reports_whether_a_channel_was_pooled() {
        let (opens, sent) = counters();
        let pool = TizenConnectionPool::with_factory(factory(opens, 0, sent));
        let client = TizenClient::new("10.0.0.4");

        assert!(!pool.evict("10.0.0.4").await);
        pool.send_key(&client, TizenRemoteKey::Power).await.unwrap();
        assert!(pool.contains("10.0.0.4").await);
        assert!(pool.evict("10.0.0.4").await);
        assert!(pool.is_empty().await);
    }

    #[tokio::test]
    async fn hosts_are_pooled_independently() {
        let (opens, sent) = counters();
        let pool = TizenConnectionPool::with_factory(factory(opens.clone(), 0, sent));

        pool.send_key(&TizenClient::new("10.0.0.5"), TizenRemoteKey::Power)
            .await
            .unwrap();
        pool.send_key(&TizenClient::new("10.0.0.6"), TizenRemoteKey::Power)
            .await
            .unwrap();

        assert_eq!(opens.load(Ordering::SeqCst), 2, "one channel per TV");
        assert_eq!(pool.len().await, 2);
    }
}
