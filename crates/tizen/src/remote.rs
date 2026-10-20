//! Stateful Samsung TV remote service.
//!
//! [`TizenRemote`] is the high-level entry point for applications that manage a
//! fleet rather than a single one-shot command. It owns the pairing-token store,
//! the per-TV connection pool and the per-host TLS trust state, so callers only
//! deal with TV addresses and commands.
//!
//! ```no_run
//! # async fn run() -> tv_rs_tizen::Result<()> {
//! use tv_rs_tizen::TizenRemote;
//!
//! let remote = TizenRemote::new("tv-tokens.json", "SignageMaster");
//! let pairing = remote.pair("192.168.1.10").await?;
//! println!("paired: token_saved={}", pairing.token_saved);
//! remote.send_key("192.168.1.10", "KEY_MUTE").await?;
//! remote.launch_app("192.168.1.10", "netflix").await?;
//! remote.shutdown().await;
//! # Ok(()) }
//! ```

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use tokio::sync::Mutex;
use tv_rs_transport::{cert_sink, CertSink, TlsPolicy};

use crate::apps::resolve_app_id;
use crate::client::TizenClient;
use crate::error::{Result, TizenError};
use crate::pool::TizenConnectionPool;
use crate::token_store::{PairedTv, SamsungTokenStore};
use crate::types::{validate_remote_key_code, TizenLaunchAction, TizenRemoteKey};

/// How an application launch was delivered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchRoute {
    /// `ms.channel.emit` / `ed.apps.launch` over the remote channel.
    Channel,
    /// DIAL `POST /ws/apps/{id}`.
    Dial,
    /// Legacy `ms.channel.launch` event.
    Legacy,
}

/// How [`TizenRemote::launch_app_with_strategy`] should choose a launch route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchStrategy {
    /// Prefer the remote channel when paired, DIAL when not, then the remaining
    /// routes as fallbacks.
    Auto,
    /// Only the remote channel.
    Channel,
    /// Only DIAL.
    Dial,
    /// Only the legacy channel event.
    Legacy,
}

/// Outcome of a pairing request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PairingOutcome {
    /// TV name reported during the handshake, when present.
    pub tv_name: Option<String>,
    /// The TV issued and the service persisted a non-empty token.
    pub token_saved: bool,
}

/// Stateful remote-control service for one or more Samsung TVs.
pub struct TizenRemote {
    tokens: Mutex<SamsungTokenStore>,
    pool: TizenConnectionPool,
    sinks: Mutex<HashMap<String, CertSink>>,
    device_name: String,
    tls_override: Option<TlsPolicy>,
    handshake_timeout: Duration,
}

impl TizenRemote {
    /// Creates a service backed by `tokens_path` (normally
    /// `<app_data>/samsung_remote/tokens.json`).
    ///
    /// TLS defaults to trust-on-first-use with one sink per TV. Persist the
    /// recorded fingerprint and switch to [`TlsPolicy::Pinned`] when the host
    /// application stores one.
    pub fn new(tokens_path: impl Into<PathBuf>, device_name: impl Into<String>) -> Self {
        Self {
            tokens: Mutex::new(SamsungTokenStore::load(tokens_path)),
            pool: TizenConnectionPool::new(),
            sinks: Mutex::new(HashMap::new()),
            device_name: device_name.into(),
            tls_override: None,
            handshake_timeout: Duration::from_secs(10),
        }
    }

    /// Uses one TLS policy for every TV instead of per-host trust-on-first-use.
    ///
    /// [`TlsPolicy::Pinned`] is the production choice once fingerprints are
    /// stored. [`TlsPolicy::AcceptInvalid`] remains debugging-only.
    pub fn with_tls_policy(mut self, policy: TlsPolicy) -> Self {
        self.tls_override = Some(policy);
        self
    }

    /// Overrides the reconnect handshake timeout (pairing still waits 30 s).
    pub fn with_handshake_timeout(mut self, timeout: Duration) -> Self {
        self.handshake_timeout = timeout;
        self
    }

    /// The token-store path this service reads and writes.
    pub async fn tokens_path(&self) -> PathBuf {
        self.tokens.lock().await.path().to_path_buf()
    }

    /// `true` when a token or a token-less legacy pairing is stored for `host`.
    pub async fn is_paired(&self, host: &str) -> bool {
        self.tokens.lock().await.get(host).is_some()
    }

    /// Stored pairings, ordered by host.
    pub async fn list_paired(&self) -> Vec<(String, PairedTv)> {
        self.tokens.lock().await.list()
    }

    /// Pairs with a TV and persists the issued token.
    ///
    /// Starts without a token so the TV displays its allow prompt and waits up
    /// to 30 seconds for a human to accept.
    pub async fn pair(&self, host: &str) -> Result<PairingOutcome> {
        let host = normalize_host(host)?;
        let pairing = self.pairing_client(&host).await.pair().await?;
        let token = pairing.token.clone().unwrap_or_default();
        let name = pairing.tv_name.clone().unwrap_or_default();
        let token_saved = !token.is_empty();
        self.tokens
            .lock()
            .await
            .upsert(&host, &token, &name)?;
        Ok(PairingOutcome {
            tv_name: pairing.tv_name,
            token_saved,
        })
    }

    /// Sends any valid Samsung `KEY_*` code to a TV.
    pub async fn send_key(&self, host: &str, code: &str) -> Result<()> {
        let host = normalize_host(host)?;
        validate_remote_key_code(code)?;
        let client = self.client_for(&host).await;
        self.pool.send_key_code(&client, code).await?;
        self.capture_pooled_token(&host).await
    }

    /// Sends one of the typed [`TizenRemoteKey`] presses.
    pub async fn send_key_typed(&self, host: &str, key: TizenRemoteKey) -> Result<()> {
        let host = normalize_host(host)?;
        let client = self.client_for(&host).await;
        self.pool.send_key(&client, key).await?;
        self.capture_pooled_token(&host).await
    }

    /// Launches an app using [`LaunchStrategy::Auto`] and a deep link.
    pub async fn launch_app(&self, host: &str, app_id: &str) -> Result<LaunchRoute> {
        self.launch_app_with_strategy(
            host,
            app_id,
            LaunchStrategy::Auto,
            TizenLaunchAction::DeepLink,
        )
        .await
    }

    /// Launches an app with an explicit channel action.
    pub async fn launch_app_with_action(
        &self,
        host: &str,
        app_id: &str,
        action: TizenLaunchAction,
    ) -> Result<LaunchRoute> {
        self.launch_app_with_strategy(host, app_id, LaunchStrategy::Auto, action)
            .await
    }

    /// Launches an app using an explicit route strategy.
    ///
    /// `app_id` accepts the presets from [`resolve_app_id`](crate::resolve_app_id)
    /// (`netflix`, `youtube`, `prime`) or a numeric Samsung app id. `Auto` tries
    /// the route most likely to work for the current pairing state, then falls
    /// back through the other routes before returning the last error.
    pub async fn launch_app_with_strategy(
        &self,
        host: &str,
        app_id: &str,
        strategy: LaunchStrategy,
        action: TizenLaunchAction,
    ) -> Result<LaunchRoute> {
        let host = normalize_host(host)?;
        let app_id = resolve_app_id(app_id)?;
        let client = self.client_for(&host).await;
        let paired = self.tokens.lock().await.token_for(&host).is_some();

        let routes: &[LaunchRoute] = match strategy {
            LaunchStrategy::Auto if paired => {
                &[LaunchRoute::Channel, LaunchRoute::Dial, LaunchRoute::Legacy]
            }
            LaunchStrategy::Auto => &[LaunchRoute::Dial, LaunchRoute::Channel, LaunchRoute::Legacy],
            LaunchStrategy::Channel => &[LaunchRoute::Channel],
            LaunchStrategy::Dial => &[LaunchRoute::Dial],
            LaunchStrategy::Legacy => &[LaunchRoute::Legacy],
        };

        let mut last_error = None;
        for route in routes {
            let result = match route {
                LaunchRoute::Channel => self.pool.launch_app(&client, &app_id, action).await,
                LaunchRoute::Dial => client
                    .launch_app_via_dial(&app_id, None)
                    .await
                    .map(|_| ()),
                LaunchRoute::Legacy => self.pool.launch_app_legacy(&client, &app_id).await,
            };
            match result {
                Ok(()) => {
                    self.capture_pooled_token(&host).await?;
                    return Ok(*route);
                }
                Err(error) => last_error = Some(error),
            }
        }

        Err(last_error.unwrap_or_else(|| {
            TizenError::Protocol("no app-launch route was attempted".to_string())
        }))
    }

    /// Drops the pooled channel for `host`; `true` when one was connected.
    pub async fn disconnect(&self, host: &str) -> bool {
        self.pool.evict(host).await
    }

    /// Removes a pairing and closes its channel; `true` when a record existed.
    pub async fn forget_paired(&self, host: &str) -> Result<bool> {
        let removed = self.tokens.lock().await.remove(host)?;
        self.pool.evict(host).await;
        Ok(removed)
    }

    /// `true` when `host` currently has a pooled channel.
    pub async fn is_connected(&self, host: &str) -> bool {
        self.pool.contains(host).await
    }

    /// Number of TVs with a pooled channel.
    pub async fn connected_count(&self) -> usize {
        self.pool.len().await
    }

    /// Closes every pooled channel. Pairings stay persisted; call this at
    /// application shutdown or before suspending remote control.
    pub async fn shutdown(&self) -> usize {
        self.pool.clear().await
    }

    async fn pairing_client(&self, host: &str) -> TizenClient {
        let client = TizenClient::new(host)
            .with_device_name(&self.device_name)
            .with_handshake_timeout(self.handshake_timeout);
        self.apply_tls(client, host).await
    }

    async fn client_for(&self, host: &str) -> TizenClient {
        let token = self
            .tokens
            .lock()
            .await
            .token_for(host)
            .map(str::to_string);
        let client = self.pairing_client(host).await;
        match token {
            Some(token) => client.with_token(token),
            None => client,
        }
    }

    async fn apply_tls(&self, client: TizenClient, host: &str) -> TizenClient {
        if let Some(policy) = &self.tls_override {
            return client.with_tls_policy(policy.clone());
        }
        let sink = self
            .sinks
            .lock()
            .await
            .entry(host.to_string())
            .or_insert_with(cert_sink)
            .clone();
        client.with_tls_policy(TlsPolicy::TrustOnFirstUse(sink))
    }

    /// Stores a token the TV issued during an ordinary pooled handshake.
    async fn capture_pooled_token(&self, host: &str) -> Result<()> {
        let Some(token) = self.pool.token_for(host).await else {
            return Ok(());
        };
        let mut store = self.tokens.lock().await;
        if store.token_for(host) == Some(token.as_str()) {
            return Ok(());
        }
        let name = store
            .get(host)
            .map(|entry| entry.name.clone())
            .unwrap_or_default();
        store.upsert(host, &token, &name)
    }
}

/// Rejects empty, URL-shaped or path-shaped TV addresses.
pub fn normalize_host(host: &str) -> Result<String> {
    let host = host.trim();
    let invalid = host.is_empty()
        || host.len() > 64
        || host.contains("://")
        || host.contains('/')
        || host.contains(' ');
    if invalid {
        return Err(TizenError::InvalidHost(host.to_string()));
    }
    Ok(host.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tv-rs-tizen-remote-{tag}-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or_default()
        ))
    }

    #[test]
    fn host_validation_rejects_urls_and_whitespace() {
        assert_eq!(normalize_host(" 192.168.1.50 ").unwrap(), "192.168.1.50");
        assert_eq!(normalize_host("tv.local").unwrap(), "tv.local");
        assert!(normalize_host("").is_err());
        assert!(normalize_host("ws://192.168.1.50").is_err());
        assert!(normalize_host("192.168.1.50/path").is_err());
        assert!(normalize_host("192.168.1.50 x").is_err());
        assert!(normalize_host(&"a".repeat(65)).is_err());
    }

    #[tokio::test]
    async fn token_lifecycle_is_owned_by_the_service() {
        let path = temp_path("lifecycle");
        let remote = TizenRemote::new(&path, "SignageMaster");
        assert!(!remote.is_paired("10.0.0.20").await);
        assert!(remote.list_paired().await.is_empty());

        remote
            .tokens
            .lock()
            .await
            .upsert("10.0.0.20", "tok", "Den TV")
            .expect("write token");
        assert!(remote.is_paired("10.0.0.20").await);
        assert_eq!(remote.list_paired().await.len(), 1);

        assert!(remote.forget_paired("10.0.0.20").await.unwrap());
        assert!(!remote.is_paired("10.0.0.20").await);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn shutdown_closes_connections_without_touching_pairings() {
        let path = temp_path("shutdown");
        let remote = TizenRemote::new(&path, "SignageMaster");
        remote
            .tokens
            .lock()
            .await
            .upsert("10.0.0.21", "tok", "Hall")
            .expect("write token");

        assert_eq!(remote.shutdown().await, 0);
        assert!(remote.is_paired("10.0.0.21").await);
        let _ = std::fs::remove_file(path);
    }
}
