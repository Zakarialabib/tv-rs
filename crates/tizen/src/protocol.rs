//! Protocol traits for the Tizen WebSocket remote channel and DIAL launcher.
//!
//! Both traits are object-safe (via `#[async_trait]`) so the `tv-rs` facade can
//! hold them as `dyn` values.

use async_trait::async_trait;

use crate::client::TizenClient;
use crate::error::Result;

/// Opens the Tizen remote WebSocket channel and captures the pairing token.
#[async_trait]
pub trait WebSocketHandshake: Send + Sync {
    /// Connects using an optional previously-saved `token`.
    ///
    /// Returns the token echoed by the TV, or `None` when the TV returned none.
    /// A denied pairing surfaces as [`crate::TizenError::Unauthorized`].
    async fn connect_ws(&self, token: Option<&str>) -> Result<Option<String>>;
}

/// Launches an application through the TV's DIAL server.
#[async_trait]
pub trait DialLauncher: Send + Sync {
    /// Launches `app_id`, optionally with a DIAL client payload.
    ///
    /// DIAL launching is fire-and-forget; success only means the TV accepted the
    /// request.
    async fn launch_app_via_dial(&self, app_id: &str, payload: Option<&str>) -> Result<()>;
}

#[async_trait]
impl DialLauncher for TizenClient {
    async fn launch_app_via_dial(&self, app_id: &str, payload: Option<&str>) -> Result<()> {
        TizenClient::launch_app_via_dial(self, app_id, payload).await?;
        Ok(())
    }
}
