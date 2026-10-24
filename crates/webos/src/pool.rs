//! A per-TV session pool for the SSAP WebSocket.
//!
//! Every webOS command today pays the full handshake twice over: the WebSocket
//! connect and the SSAP `register`. Worse, connecting without a stored
//! `client-key` triggers the on-screen "Allow?" prompt — so a player that opens
//! a session per command turns every volume change into an interrupted show.
//! The pool keeps one registered session per TV and reopens it exactly once
//! when a request fails against it.
//!
//! ```
//! # async fn demo() -> tv_rs_webos::Result<()> {
//! use tv_rs_webos::{WebosClient, WebosSessionPool};
//!
//! let pool = WebosSessionPool::new();
//! let client = WebosClient::new("192.168.1.20")
//!     .with_wss()
//!     .with_client_key("persisted-key");
//!
//! pool.set_volume(&client, 20).await?;
//! pool.set_volume(&client, 40).await?; // reuses the registered session
//! assert_eq!(pool.len().await, 1);
//! pool.evict("192.168.1.20").await;
//! # Ok(()) }
//! ```
//!
//! A session that has been idle for longer than the pool's idle limit is
//! dropped instead of reused: the next command opens a fresh one. TVs do drop
//! idle sockets, and reusing a stale session is harmless (the reopen-once
//! retry covers it), but shedding the socket is friendlier to the TV than
//! keeping it alive forever.
//!
//! The pool serialises requests per TV, which matches the protocol: one
//! session, one request at a time.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Mutex;

use crate::client::{WebosClient, WebosSession};
use crate::error::Result;
use crate::pointer::{
    extract_socket_path, BoxedPointerFactory, LivePointerFactory, PointerFrame, PointerWire,
};
use crate::types::SsapUri;

/// Sessions idle for at least this long are dropped and opened fresh on the
/// next command.
const DEFAULT_IDLE_AFTER: Duration = Duration::from_secs(60);

/// A registered SSAP session that can serve requests.
///
/// Implemented for [`WebosSession`]; the trait exists so the pool can be
/// tested without a television, and so a pooled session can later live inside
/// a richer reconnecting wrapper.
#[async_trait]
pub trait WebosSessionWire: Send {
    /// Sends `uri` with a fresh request id and returns the matching `payload`.
    async fn request(&mut self, uri: SsapUri, payload: Option<Value>) -> Result<Value>;

    /// Opens a URL in the TV's built-in browser.
    async fn open_url(&mut self, url: &str) -> Result<()>;

    /// Turns the TV off.
    async fn power_off(&mut self) -> Result<()>;

    /// Launches an installed application by id.
    async fn launch_app(&mut self, app_id: &str) -> Result<()>;

    /// Reads the current output volume.
    async fn get_volume(&mut self) -> Result<u8>;

    /// Sets the output volume, clamped to `0..=100`.
    async fn set_volume(&mut self, volume: u8) -> Result<()>;

    /// Enables or disables mute.
    async fn set_mute(&mut self, mute: bool) -> Result<()>;

    /// Returns the id of the foreground application, when the TV reports one.
    async fn foreground_app(&mut self) -> Result<Option<String>>;

    /// The `client-key` associated with this session, when one is known.
    fn client_key(&self) -> Option<&str> {
        None
    }
}

#[async_trait]
impl WebosSessionWire for WebosSession {
    async fn request(&mut self, uri: SsapUri, payload: Option<Value>) -> Result<Value> {
        WebosSession::request(self, uri, payload).await
    }

    async fn open_url(&mut self, url: &str) -> Result<()> {
        WebosSession::open_url(self, url).await
    }

    async fn power_off(&mut self) -> Result<()> {
        WebosSession::power_off(self).await
    }

    async fn launch_app(&mut self, app_id: &str) -> Result<()> {
        WebosSession::launch_app(self, app_id).await
    }

    async fn get_volume(&mut self) -> Result<u8> {
        WebosSession::get_volume(self).await
    }

    async fn set_volume(&mut self, volume: u8) -> Result<()> {
        WebosSession::set_volume(self, volume).await
    }

    async fn set_mute(&mut self, mute: bool) -> Result<()> {
        WebosSession::set_mute(self, mute).await
    }

    async fn foreground_app(&mut self) -> Result<Option<String>> {
        WebosSession::foreground_app(self).await
    }

    fn client_key(&self) -> Option<&str> {
        WebosSession::client_key(self)
    }
}

/// Opens sessions for the pool.
#[async_trait]
pub trait WebosSessionFactory: Send + Sync {
    /// Opens a fresh registered session for `client`.
    async fn open(&self, client: &WebosClient) -> Result<Box<dyn WebosSessionWire>>;
}

/// The production factory: a real WebSocket connect plus SSAP registration.
#[derive(Debug, Clone, Copy, Default)]
pub struct LiveSessionFactory;

#[async_trait]
impl WebosSessionFactory for LiveSessionFactory {
    async fn open(&self, client: &WebosClient) -> Result<Box<dyn WebosSessionWire>> {
        Ok(Box::new(client.connect().await?))
    }
}

/// One command to run through the pool.
#[derive(Clone)]
enum Op<'a> {
    Request(SsapUri, Option<Value>),
    OpenUrl(&'a str),
    PowerOff,
    LaunchApp(&'a str),
    GetVolume,
    SetVolume(u8),
    SetMute(bool),
    ForegroundApp,
}

/// What one command produced, before the typed pool methods unwrap it.
enum OpOutcome {
    Value(Value),
    Volume(u8),
    Foreground(Option<String>),
    Done,
}

impl Op<'_> {
    async fn run(&self, wire: &mut dyn WebosSessionWire) -> Result<OpOutcome> {
        match self {
            Op::Request(uri, payload) => {
                Ok(OpOutcome::Value(wire.request(*uri, payload.clone()).await?))
            }
            Op::OpenUrl(url) => {
                wire.open_url(url).await?;
                Ok(OpOutcome::Done)
            }
            Op::PowerOff => {
                wire.power_off().await?;
                Ok(OpOutcome::Done)
            }
            Op::LaunchApp(app_id) => {
                wire.launch_app(app_id).await?;
                Ok(OpOutcome::Done)
            }
            Op::GetVolume => Ok(OpOutcome::Volume(wire.get_volume().await?)),
            Op::SetVolume(volume) => {
                wire.set_volume(*volume).await?;
                Ok(OpOutcome::Done)
            }
            Op::SetMute(mute) => {
                wire.set_mute(*mute).await?;
                Ok(OpOutcome::Done)
            }
            Op::ForegroundApp => Ok(OpOutcome::Foreground(wire.foreground_app().await?)),
        }
    }
}

/// A pooled session, with the time of its last successful command.
struct PooledSession {
    session: Box<dyn WebosSessionWire>,
    last_used: Instant,
}

impl PooledSession {
    fn new(session: Box<dyn WebosSessionWire>) -> Self {
        Self {
            session,
            last_used: Instant::now(),
        }
    }

    fn mark_used(&mut self) {
        self.last_used = Instant::now();
    }

    fn idle_expired(&self, limit: Duration) -> bool {
        self.last_used.elapsed() >= limit
    }
}

/// A pooled pointer socket, with the time of its last successful frame.
///
/// Held separately from the session map because it is a *different*
/// connection: the SSAP session serves requests, the pointer socket takes
/// plain-text frames. Both are per-TV and both are reused.
struct PooledPointer {
    socket: Box<dyn PointerWire>,
    last_used: Instant,
}

impl PooledPointer {
    fn new(socket: Box<dyn PointerWire>) -> Self {
        Self {
            socket,
            last_used: Instant::now(),
        }
    }

    fn mark_used(&mut self) {
        self.last_used = Instant::now();
    }

    fn idle_expired(&self, limit: Duration) -> bool {
        self.last_used.elapsed() >= limit
    }
}

/// One registered session per TV, keyed by host.
pub struct WebosSessionPool<F: WebosSessionFactory = LiveSessionFactory> {
    factory: F,
    /// Pointer sockets are always factory-erased: they are an implementation
    /// detail of key input, and giving the pool a second type parameter would
    /// leak into every facade that stores a `DynSessionPool`.
    pointer_factory: BoxedPointerFactory,
    idle_after: Duration,
    sessions: Mutex<HashMap<String, PooledSession>>,
    pointers: Mutex<HashMap<String, PooledPointer>>,
}

impl WebosSessionPool<LiveSessionFactory> {
    /// Creates a pool backed by real registrations, with the default idle
    /// limit.
    pub fn new() -> Self {
        Self::with_factory(LiveSessionFactory)
    }
}

/// A session factory behind a box, so a pool can hold a factory it does not name.
///
/// This is what lets a facade store one pool type while still accepting a
/// caller-supplied factory — a different transport, or a test double.
pub type BoxedSessionFactory = Box<dyn WebosSessionFactory + Send + Sync>;

#[async_trait]
impl WebosSessionFactory for BoxedSessionFactory {
    async fn open(&self, client: &WebosClient) -> Result<Box<dyn WebosSessionWire>> {
        (**self).open(client).await
    }
}

/// The pool with its factory erased: the type a facade can store.
///
/// [`WebosSessionPool::new`] gives the live factory; a caller that wants its own
/// builds one with `DynSessionPool::with_factory(Box::new(their_factory))`.
pub type DynSessionPool = WebosSessionPool<BoxedSessionFactory>;

impl Default for WebosSessionPool<LiveSessionFactory> {
    fn default() -> Self {
        Self::new()
    }
}

impl<F: WebosSessionFactory> WebosSessionPool<F> {
    /// Creates a pool with a custom session factory (tests, instrumentation),
    /// using the default idle limit.
    pub fn with_factory(factory: F) -> Self {
        Self::with_factory_and_idle(factory, DEFAULT_IDLE_AFTER)
    }

    /// Creates a pool with a custom factory and idle limit. An idle limit of
    /// zero drops every session after each command.
    pub fn with_factory_and_idle(factory: F, idle_after: Duration) -> Self {
        Self::with_factories_and_idle(factory, Box::new(LivePointerFactory), idle_after)
    }

    /// Creates a pool with a custom session factory, a custom pointer factory
    /// and an idle limit.
    ///
    /// Tests use this to count pointer dials: a television cannot be asked to
    /// drop a pointer socket on cue, so "reopened exactly once" is only
    /// observable through a factory that counts.
    pub fn with_factories_and_idle(
        factory: F,
        pointer_factory: BoxedPointerFactory,
        idle_after: Duration,
    ) -> Self {
        Self {
            factory,
            pointer_factory,
            idle_after,
            sessions: Mutex::new(HashMap::new()),
            pointers: Mutex::new(HashMap::new()),
        }
    }

    /// Sends a raw SSAP request through the pooled session.
    pub async fn request(
        &self,
        client: &WebosClient,
        uri: SsapUri,
        payload: Option<Value>,
    ) -> Result<Value> {
        take_value(self.dispatch(client, &Op::Request(uri, payload)).await)
    }

    /// Opens a URL through the pooled session.
    pub async fn open_url(&self, client: &WebosClient, url: &str) -> Result<()> {
        self.dispatch(client, &Op::OpenUrl(url)).await?;
        Ok(())
    }

    /// Turns the TV off through the pooled session.
    pub async fn power_off(&self, client: &WebosClient) -> Result<()> {
        self.dispatch(client, &Op::PowerOff).await?;
        Ok(())
    }

    /// Launches an app through the pooled session.
    pub async fn launch_app(&self, client: &WebosClient, app_id: &str) -> Result<()> {
        self.dispatch(client, &Op::LaunchApp(app_id)).await?;
        Ok(())
    }

    /// Reads the output volume through the pooled session.
    pub async fn get_volume(&self, client: &WebosClient) -> Result<u8> {
        match self.dispatch(client, &Op::GetVolume).await? {
            OpOutcome::Volume(volume) => Ok(volume),
            _ => unreachable!("volume op yields Volume"),
        }
    }

    /// Sets the output volume through the pooled session.
    pub async fn set_volume(&self, client: &WebosClient, volume: u8) -> Result<()> {
        self.dispatch(client, &Op::SetVolume(volume)).await?;
        Ok(())
    }

    /// Enables or disables mute through the pooled session.
    pub async fn set_mute(&self, client: &WebosClient, mute: bool) -> Result<()> {
        self.dispatch(client, &Op::SetMute(mute)).await?;
        Ok(())
    }

    /// Reads the foreground application through the pooled session.
    pub async fn foreground_app(&self, client: &WebosClient) -> Result<Option<String>> {
        match self.dispatch(client, &Op::ForegroundApp).await? {
            OpOutcome::Foreground(app) => Ok(app),
            _ => unreachable!("foreground op yields Foreground"),
        }
    }

    /// Sends one remote-key press over the TV's pointer socket.
    ///
    /// The pointer channel is a *second*, per-session WebSocket: the path comes
    /// from `getPointerInputSocket` on the pooled SSAP session, and the frames
    /// are plain text (`pointer.rs`). One pointer socket is kept per TV and
    /// reused across presses — dialing costs a full WebSocket handshake, and a
    /// remote that re-dialed on every key would be unusable. A dead socket is
    /// reopened exactly once, like a dead session.
    ///
    /// `name` is the wire button name (`HOME`, `ENTER`, `VOLUMEUP`, …). The
    /// vendor vocabulary is the caller's job: this crate does not depend on
    /// `tv-rs-core`, so the facade owns the key map.
    pub async fn send_key(&self, client: &WebosClient, name: &str) -> Result<()> {
        let host = client.host().to_string();
        let frame = PointerFrame::Button(name).encode();

        let mut pointers = self.pointers.lock().await;
        let idle_expired = pointers
            .get(&host)
            .is_some_and(|pointer| pointer.idle_expired(self.idle_after));
        if idle_expired || !pointers.contains_key(&host) {
            pointers.remove(&host);
            let socket = self.open_pointer(client).await?;
            pointers.insert(host.clone(), PooledPointer::new(socket));
        }

        let pooled = pointers
            .get_mut(&host)
            .expect("pointer was just inserted or already present");

        let first = pooled.socket.send_frame(frame.clone()).await;
        if first.is_ok() {
            pooled.mark_used();
            return first;
        }

        // Dead pointer socket (idle timeout, TV rebooted): reopen once, retry.
        pointers.remove(&host);
        let mut fresh = self.open_pointer(client).await?;
        let retry = fresh.send_frame(frame).await;
        let mut pooled = PooledPointer::new(fresh);
        if retry.is_ok() {
            pooled.mark_used();
        }
        pointers.insert(host, pooled);
        retry
    }

    /// Asks the pooled SSAP session for a pointer path and dials it.
    ///
    /// The path is per-session, so it is fetched through the same session that
    /// serves every other command — and re-fetched whenever the pointer socket
    /// has to be replaced.
    async fn open_pointer(&self, client: &WebosClient) -> Result<Box<dyn PointerWire>> {
        let payload = self
            .request(client, SsapUri::NetworkInputGetPointerSocket, None)
            .await?;
        let socket_path = extract_socket_path(&payload)?;
        self.pointer_factory
            .open(client.tls_policy(), &socket_path)
            .await
    }

    /// Drops the pooled session for `host`; `true` when one was connected.
    ///
    /// Call this when a TV is removed from the fleet, or after
    /// [`power_off`](Self::power_off) — the next command opens a fresh
    /// session. Any pooled pointer socket for the same host goes with it: its
    /// path belongs to the session that issued it.
    pub async fn evict(&self, host: &str) -> bool {
        self.pointers.lock().await.remove(host);
        self.sessions.lock().await.remove(host).is_some()
    }

    /// Number of TV sessions currently pooled.
    pub async fn len(&self) -> usize {
        self.sessions.lock().await.len()
    }

    /// `true` when nothing is pooled.
    pub async fn is_empty(&self) -> bool {
        self.sessions.lock().await.is_empty()
    }

    /// `true` when `host` has a pooled session.
    pub async fn contains(&self, host: &str) -> bool {
        self.sessions.lock().await.contains_key(host)
    }

    /// `true` when `host` has a pooled pointer socket.
    ///
    /// Separate from [`contains`](Self::contains): a TV can hold a registered
    /// session with no pointer socket yet, which is the state before the first
    /// key press.
    pub async fn has_pointer(&self, host: &str) -> bool {
        self.pointers.lock().await.contains_key(host)
    }

    /// `client-key` observed on the pooled session for `host`, when known.
    pub async fn client_key_for(&self, host: &str) -> Option<String> {
        self.sessions
            .lock()
            .await
            .get(host)
            .and_then(|pooled| pooled.session.client_key())
            .map(str::to_string)
    }

    /// Drops every pooled session and pointer socket; returns how many sessions
    /// were connected.
    ///
    /// A mobile caller needs this before the app is backgrounded: the OS will
    /// not close a WebSocket for it, and both the SSAP session and the pointer
    /// socket count. The pointer path belongs to the session that issued it, so
    /// they go together.
    pub async fn clear(&self) -> usize {
        self.pointers.lock().await.clear();
        let mut sessions = self.sessions.lock().await;
        let count = sessions.len();
        sessions.clear();
        count
    }

    async fn dispatch(&self, client: &WebosClient, op: &Op<'_>) -> Result<OpOutcome> {
        let host = client.host().to_string();

        let mut sessions = self.sessions.lock().await;
        let idle_expired = sessions
            .get(&host)
            .is_some_and(|pooled| pooled.idle_expired(self.idle_after));
        if idle_expired || !sessions.contains_key(&host) {
            sessions.remove(&host);
            let session = self.factory.open(client).await?;
            sessions.insert(host.clone(), PooledSession::new(session));
        }

        let pooled = sessions
            .get_mut(&host)
            .expect("session was just inserted or already present");

        let first = op.run(pooled.session.as_mut()).await;
        if first.is_ok() {
            pooled.mark_used();
            return first;
        }

        // Dead session (idle timeout, TV rebooted): reopen once and retry.
        sessions.remove(&host);
        let mut fresh = self.factory.open(client).await?;
        let retry = op.run(fresh.as_mut()).await;
        let mut pooled = PooledSession::new(fresh);
        if retry.is_ok() {
            pooled.mark_used();
        }
        sessions.insert(host, pooled);
        retry
    }
}

/// Unwraps a request outcome, which is the only value-returning op.
fn take_value(outcome: Result<OpOutcome>) -> Result<Value> {
    match outcome? {
        OpOutcome::Value(value) => Ok(value),
        _ => unreachable!("request op yields Value"),
    }
}

#[cfg(any(test, feature = "testkit"))]
pub mod testkit {
    //! Counting test doubles for [`WebosSessionPool`].
    //!
    //! The pool behaviour that matters — one registered session reused across
    //! commands, a dead session reopened exactly once, an idle session dropped —
    //! is only observable through a factory that counts opens. A television
    //! cannot be asked to drop a socket on cue, so the counting factory lives
    //! here: this crate's own tests use it, and any downstream crate that enables
    //! the `testkit` feature gets the same honest double.
    //!
    //! Nothing in this module touches a network.

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex as StdMutex};

    use async_trait::async_trait;
    use serde_json::{json, Value};

    use super::{WebosSessionFactory, WebosSessionWire};
    use crate::client::WebosClient;
    use crate::error::{Result, WebosError};
    use crate::pointer::{BoxedPointerFactory, PointerFactory, PointerWire};
    use crate::types::SsapUri;

    /// A session that spends from a shared budget of simulated failures.
    ///
    /// A shared budget is what makes the retry observable: the first session
    /// fails, the reopened one succeeds.
    pub struct FakeSession {
        failures: Arc<StdMutex<usize>>,
        served: Arc<AtomicUsize>,
        volume: u8,
    }

    #[async_trait]
    impl WebosSessionWire for FakeSession {
        async fn request(&mut self, uri: SsapUri, _payload: Option<Value>) -> Result<Value> {
            self.serve()?;
            // A real TV answers `getPointerInputSocket` with a complete
            // WebSocket URL. Anything else is `Value::Null`, which the pool
            // never inspects.
            Ok(match uri {
                SsapUri::NetworkInputGetPointerSocket => json!({
                    "socketPath": "wss://10.0.0.9:3001/resources/h/netinput.pointer.sock"
                }),
                _ => Value::Null,
            })
        }

        async fn open_url(&mut self, _url: &str) -> Result<()> {
            self.serve()
        }

        async fn power_off(&mut self) -> Result<()> {
            self.serve()
        }

        async fn launch_app(&mut self, _app_id: &str) -> Result<()> {
            self.serve()
        }

        async fn get_volume(&mut self) -> Result<u8> {
            self.serve()?;
            Ok(self.volume)
        }

        async fn set_volume(&mut self, _volume: u8) -> Result<()> {
            self.serve()
        }

        async fn set_mute(&mut self, _mute: bool) -> Result<()> {
            self.serve()
        }

        async fn foreground_app(&mut self) -> Result<Option<String>> {
            self.serve()?;
            Ok(Some("netflix".to_string()))
        }
    }

    impl FakeSession {
        /// Spends one simulated failure or records one served command.
        fn serve(&mut self) -> Result<()> {
            let mut budget = self.failures.lock().expect("test mutex");
            if *budget > 0 {
                *budget -= 1;
                return Err(WebosError::Protocol("dead session".to_string()));
            }
            self.served.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    /// A factory that counts opens and hands out [`FakeSession`]s.
    pub struct FakeFactory {
        opens: Arc<AtomicUsize>,
        failures: Arc<StdMutex<usize>>,
        served: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl WebosSessionFactory for FakeFactory {
        async fn open(&self, _client: &WebosClient) -> Result<Box<dyn WebosSessionWire>> {
            self.opens.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakeSession {
                failures: self.failures.clone(),
                served: self.served.clone(),
                volume: 42,
            }))
        }
    }

    /// `(opens, served)` counters, both starting at zero.
    #[must_use]
    pub fn counters() -> (Arc<AtomicUsize>, Arc<AtomicUsize>) {
        (Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)))
    }

    /// A factory that opens `failures` dead sessions before serving.
    #[must_use]
    pub fn factory(
        opens: Arc<AtomicUsize>,
        failures: usize,
        served: Arc<AtomicUsize>,
    ) -> FakeFactory {
        FakeFactory {
            opens,
            failures: Arc::new(StdMutex::new(failures)),
            served,
        }
    }

    /// A pointer socket that spends from a shared budget of simulated failures
    /// and records every frame it was asked to send.
    ///
    /// The recorded frames are the point: they prove the *wire* form of a key
    /// press, not just that something was written.
    pub struct FakePointer {
        failures: Arc<StdMutex<usize>>,
        frames: Arc<StdMutex<Vec<String>>>,
    }

    #[async_trait]
    impl PointerWire for FakePointer {
        async fn send_frame(&mut self, frame: String) -> Result<()> {
            let mut budget = self.failures.lock().expect("test mutex");
            if *budget > 0 {
                *budget -= 1;
                return Err(WebosError::Protocol("dead pointer".to_string()));
            }
            drop(budget);
            self.frames.lock().expect("test mutex").push(frame);
            Ok(())
        }
    }

    /// A pointer factory that counts dials and hands out [`FakePointer`]s.
    pub struct FakePointerFactory {
        opens: Arc<AtomicUsize>,
        failures: Arc<StdMutex<usize>>,
        frames: Arc<StdMutex<Vec<String>>>,
    }

    #[async_trait]
    impl PointerFactory for FakePointerFactory {
        async fn open(
            &self,
            _tls_policy: crate::TlsPolicy,
            _socket_path: &str,
        ) -> Result<Box<dyn PointerWire>> {
            self.opens.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(FakePointer {
                failures: self.failures.clone(),
                frames: self.frames.clone(),
            }))
        }
    }

    /// A pointer factory plus the counters a test asserts on.
    ///
    /// `failures` is how many `send_frame` calls die before one serves, which is
    /// how a dead pointer socket is simulated.
    #[must_use]
    pub fn pointer_factory(
        failures: usize,
    ) -> (
        BoxedPointerFactory,
        Arc<AtomicUsize>,
        Arc<StdMutex<Vec<String>>>,
    ) {
        let opens = Arc::new(AtomicUsize::new(0));
        let frames = Arc::new(StdMutex::new(Vec::new()));
        let factory = FakePointerFactory {
            opens: opens.clone(),
            failures: Arc::new(StdMutex::new(failures)),
            frames: frames.clone(),
        };
        (Box::new(factory), opens, frames)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use serde_json::json;

    use super::testkit::{counters, factory, pointer_factory};
    use super::*;

    #[tokio::test]
    async fn reuses_one_session_for_repeated_commands() {
        let (opens, served) = counters();
        let pool = WebosSessionPool::with_factory(factory(opens.clone(), 0, served.clone()));
        let client = WebosClient::new("10.0.0.1");

        pool.set_volume(&client, 10).await.unwrap();
        pool.set_mute(&client, true).await.unwrap();
        pool.launch_app(&client, "netflix").await.unwrap();

        assert_eq!(opens.load(Ordering::SeqCst), 1, "session must be reused");
        assert_eq!(served.load(Ordering::SeqCst), 3);
        assert_eq!(pool.len().await, 1);
    }

    #[tokio::test]
    async fn key_presses_reuse_one_pointer_socket() {
        let (opens, served) = counters();
        let (pointer_factory, pointer_opens, frames) = pointer_factory(0);
        let pool = WebosSessionPool::with_factories_and_idle(
            factory(opens.clone(), 0, served.clone()),
            pointer_factory,
            Duration::from_secs(60),
        );
        let client = WebosClient::new("10.0.0.5");

        pool.send_key(&client, "HOME").await.unwrap();
        pool.send_key(&client, "DOWN").await.unwrap();

        // Two presses, one dial: a remote that re-dialed per key would be
        // unusable, and the TV issues a fresh path per request.
        assert_eq!(pointer_opens.load(Ordering::SeqCst), 1, "one dial");
        assert!(pool.has_pointer("10.0.0.5").await);
        assert_eq!(
            frames.lock().expect("test mutex").as_slice(),
            ["type:button\nname:HOME\n\n", "type:button\nname:DOWN\n\n"],
            "the wire form of a press"
        );
    }

    #[tokio::test]
    async fn a_dead_pointer_socket_is_reopened_once() {
        let (opens, served) = counters();
        let (pointer_factory, pointer_opens, frames) = pointer_factory(1);
        let pool = WebosSessionPool::with_factories_and_idle(
            factory(opens.clone(), 0, served.clone()),
            pointer_factory,
            Duration::from_secs(60),
        );
        let client = WebosClient::new("10.0.0.6");

        pool.send_key(&client, "ENTER").await.unwrap();

        assert_eq!(pointer_opens.load(Ordering::SeqCst), 2, "one reopen");
        assert_eq!(
            frames.lock().expect("test mutex").as_slice(),
            ["type:button\nname:ENTER\n\n"]
        );
    }

    #[tokio::test]
    async fn clearing_the_pool_drops_the_pointer_socket_too() {
        let (opens, served) = counters();
        let (pointer_factory, _, _) = pointer_factory(0);
        let pool = WebosSessionPool::with_factories_and_idle(
            factory(opens.clone(), 0, served.clone()),
            pointer_factory,
            Duration::from_secs(60),
        );
        let client = WebosClient::new("10.0.0.7");

        pool.send_key(&client, "HOME").await.unwrap();
        assert!(pool.has_pointer("10.0.0.7").await);

        // Mobile backgrounding: the OS will not close the socket for us.
        pool.clear().await;
        assert!(!pool.has_pointer("10.0.0.7").await);
        assert!(pool.is_empty().await);
    }

    #[tokio::test]
    async fn a_dead_session_is_reopened_once_and_the_command_retried() {
        let (opens, served) = counters();
        let pool = WebosSessionPool::with_factory(factory(opens.clone(), 1, served.clone()));
        let client = WebosClient::new("10.0.0.2");

        pool.set_volume(&client, 20).await.unwrap();

        assert_eq!(opens.load(Ordering::SeqCst), 2, "one reopen");
        assert_eq!(
            served.load(Ordering::SeqCst),
            1,
            "retry delivered exactly once"
        );
    }

    #[tokio::test]
    async fn a_second_failure_is_returned_to_the_caller() {
        let (opens, served) = counters();
        let pool = WebosSessionPool::with_factory(factory(opens.clone(), 2, served.clone()));
        let client = WebosClient::new("10.0.0.3");

        let result = pool.set_volume(&client, 30).await;

        assert!(result.is_err(), "two failures must surface");
        assert_eq!(opens.load(Ordering::SeqCst), 2, "retry is not a loop");
        assert_eq!(served.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn an_idle_session_is_dropped_and_reopened_on_next_use() {
        let (opens, served) = counters();
        let pool = WebosSessionPool::with_factory_and_idle(
            factory(opens.clone(), 0, served.clone()),
            Duration::ZERO,
        );
        let client = WebosClient::new("10.0.0.4");

        pool.set_volume(&client, 10).await.unwrap();
        assert_eq!(pool.len().await, 1);
        pool.set_volume(&client, 20).await.unwrap();

        assert_eq!(opens.load(Ordering::SeqCst), 2, "zero idle limit expires");
    }

    #[tokio::test]
    async fn value_reads_flow_through_the_pool() {
        let (opens, served) = counters();
        let pool = WebosSessionPool::with_factory(factory(opens.clone(), 0, served.clone()));
        let client = WebosClient::new("10.0.0.5");

        let volume = pool.get_volume(&client).await.unwrap();
        let app = pool.foreground_app(&client).await.unwrap();
        let payload = pool
            .request(&client, SsapUri::SystemLauncherForegroundApp, None)
            .await
            .unwrap();

        assert_eq!(volume, 42);
        assert_eq!(app.as_deref(), Some("netflix"));
        assert_eq!(payload, json!(null));
        assert_eq!(opens.load(Ordering::SeqCst), 1, "one session reused");
        assert_eq!(served.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn evict_reports_whether_a_session_was_pooled() {
        let (opens, served) = counters();
        let pool = WebosSessionPool::with_factory(factory(opens, 0, served));
        let client = WebosClient::new("10.0.0.6");

        assert!(!pool.evict("10.0.0.6").await);
        pool.open_url(&client, "https://example.com").await.unwrap();
        assert!(pool.contains("10.0.0.6").await);
        assert!(pool.evict("10.0.0.6").await);
        assert!(pool.is_empty().await);
    }

    #[tokio::test]
    async fn clear_drops_every_pooled_session() {
        let (opens, served) = counters();
        let pool = WebosSessionPool::with_factory(factory(opens, 0, served));

        pool.set_volume(&WebosClient::new("10.0.0.7"), 10)
            .await
            .unwrap();
        pool.set_volume(&WebosClient::new("10.0.0.8"), 40)
            .await
            .unwrap();

        assert_eq!(pool.clear().await, 2);
        assert!(pool.is_empty().await);
    }
}
