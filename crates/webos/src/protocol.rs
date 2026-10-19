//! The object-safe SSAP control trait and its implementation for [`WebosSession`].

use async_trait::async_trait;

use crate::client::WebosSession;
use crate::error::Result;
use crate::types::SsapUri;

/// A dyn-compatible surface for issuing SSAP commands against a live session.
///
/// This is implemented for [`WebosSession`] and exists so the facade crate can
/// store `Box<dyn SsapController>` values.
#[async_trait]
pub trait SsapController: Send + Sync {
    /// Sends `uri` with an optional payload and returns the response payload.
    async fn request(
        &mut self,
        uri: SsapUri,
        payload: Option<serde_json::Value>,
    ) -> Result<serde_json::Value>;

    /// Launches an installed application by id.
    async fn launch_app(&mut self, app_id: &str) -> Result<()>;
}

#[async_trait]
impl SsapController for WebosSession {
    async fn request(
        &mut self,
        uri: SsapUri,
        payload: Option<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        WebosSession::request(self, uri, payload).await
    }

    async fn launch_app(&mut self, app_id: &str) -> Result<()> {
        WebosSession::launch_app(self, app_id).await
    }
}
