//! The Roku ECP REST protocol trait.

use async_trait::async_trait;

use crate::client::RokuClient;
use crate::error::Result;
use crate::payload::{RokuApp, RokuDeviceInfo};
use crate::types::RokuKey;

/// The command surface of the Roku External Control Protocol REST API.
///
/// The trait is object-safe (`dyn EcpRestClient`) so the `tv-rs` facade can hold
/// any vendor behind one uniform interface.
#[async_trait]
pub trait EcpRestClient: Send + Sync {
    /// Fetches and parses `GET /query/device-info`.
    async fn query_device_info(&self) -> Result<RokuDeviceInfo>;

    /// Fetches and parses the installed application list from `GET /query/apps`.
    async fn query_apps(&self) -> Result<Vec<RokuApp>>;

    /// Sends a single remote-key press via `POST /keypress/{key}`.
    async fn send_keypress(&self, key: RokuKey) -> Result<()>;

    /// Launches an application by id via `POST /launch/{app_id}`.
    async fn launch_app(&self, app_id: &str) -> Result<()>;
}

#[async_trait]
impl EcpRestClient for RokuClient {
    async fn query_device_info(&self) -> Result<RokuDeviceInfo> {
        RokuClient::query_device_info(self).await
    }

    async fn query_apps(&self) -> Result<Vec<RokuApp>> {
        RokuClient::query_apps(self).await
    }

    async fn send_keypress(&self, key: RokuKey) -> Result<()> {
        RokuClient::send_keypress(self, key).await
    }

    async fn launch_app(&self, app_id: &str) -> Result<()> {
        RokuClient::launch_app(self, app_id).await
    }
}
