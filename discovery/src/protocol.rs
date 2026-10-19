//! The [`SsdpDiscovery`] trait and its implementation for [`SsdpScanner`].

use async_trait::async_trait;

use crate::client::SsdpScanner;
use crate::error::Result;
use crate::payload::SsdpMsearchResponse;

/// Object-safe SSDP discovery interface.
///
/// This is the trait the workspace facade depends on, so it must stay
/// dyn-compatible; that is why it is annotated with `#[async_trait]`.
#[async_trait]
pub trait SsdpDiscovery: Send + Sync {
    /// Discover SSDP devices on the local network.
    async fn discover(&self) -> Result<Vec<SsdpMsearchResponse>>;
}

#[async_trait]
impl SsdpDiscovery for SsdpScanner {
    async fn discover(&self) -> Result<Vec<SsdpMsearchResponse>> {
        SsdpScanner::discover(self).await
    }
}
