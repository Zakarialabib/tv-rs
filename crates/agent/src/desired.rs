//! Desired state and the seam a CMS plugs into.
//!
//! The agent does not decide what a screen should show. It asks a
//! [`DesiredStateProvider`] and converges the device towards the answer. Keeping that
//! behind a trait is what lets the same reconciler run against a TOML file, an HTTP
//! manifest or an MQTT topic without the core knowing any of them exist.

use std::collections::HashMap;
use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tv_rs_core::{AppRef, DeviceDescriptor};

/// What a device should be doing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesiredState {
    /// Power.
    pub power: DesiredPower,
    /// Application that should be in front, when one is required.
    #[serde(default)]
    pub app: Option<String>,
}

impl Default for DesiredState {
    fn default() -> Self {
        Self {
            power: DesiredPower::On,
            app: None,
        }
    }
}

impl DesiredState {
    /// A device that should be awake with `app` in front.
    #[must_use]
    pub fn showing(app: impl Into<String>) -> Self {
        Self {
            power: DesiredPower::On,
            app: Some(app.into()),
        }
    }

    /// The application reference, when one is required.
    #[must_use]
    pub fn app_ref(&self) -> Option<AppRef> {
        self.app.as_deref().map(AppRef::new)
    }
}

/// What the power state should be.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesiredPower {
    /// Awake and drawing.
    On,
    /// Off or in standby. The agent will not force `Key::Power` to get here.
    Off,
}

/// Where desired state comes from.
#[async_trait]
pub trait DesiredStateProvider: Send + Sync {
    /// The state `device` should be in, or `None` when this provider has no opinion
    /// — which means "leave it alone", not "turn it off".
    async fn desired_state(
        &self,
        device: &DeviceDescriptor,
    ) -> Result<Option<DesiredState>, ProviderError>;
}

/// Why a provider could not answer.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// The backing store could not be read.
    #[error("provider unavailable: {0}")]
    Unavailable(String),
    /// The stored state does not match the expected shape.
    #[error("provider returned invalid state: {0}")]
    Invalid(String),
}

/// A provider backed by a JSON file mapping device id to desired state.
///
/// This is the single-site and test provider: it is deliberately dumb, and it only
/// exists so that "run the agent against a file" needs no service.
#[derive(Debug)]
pub struct StaticFileProvider {
    path: std::path::PathBuf,
}

impl StaticFileProvider {
    /// Reads from `path` on every call, so editing the file is the control plane.
    #[must_use]
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The file this provider reads.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[async_trait]
impl DesiredStateProvider for StaticFileProvider {
    async fn desired_state(
        &self,
        device: &DeviceDescriptor,
    ) -> Result<Option<DesiredState>, ProviderError> {
        let text = tokio::fs::read_to_string(&self.path)
            .await
            .map_err(|error| ProviderError::Unavailable(error.to_string()))?;
        let map: HashMap<String, DesiredState> = serde_json::from_str(&text)
            .map_err(|error| ProviderError::Invalid(error.to_string()))?;
        Ok(map.get(device.id.as_str()).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tv_rs_core::{DeviceId, DeviceOs};

    fn device() -> DeviceDescriptor {
        DeviceDescriptor::new(DeviceId::new("tv-1").unwrap(), DeviceOs::Roku, "10.0.0.5")
    }

    #[tokio::test]
    async fn a_file_provider_answers_per_device_and_returns_none_for_others() {
        let dir = std::env::temp_dir().join(format!("tv-rs-agent-{}", std::process::id()));
        tokio::fs::create_dir_all(&dir).await.unwrap();
        let path = dir.join("desired.json");
        tokio::fs::write(
            &path,
            r#"{"tv-1":{"power":"on","app":"netflix"},"tv-2":{"power":"off"}}"#,
        )
        .await
        .unwrap();

        let provider = StaticFileProvider::new(&path);
        let desired = provider
            .desired_state(&device())
            .await
            .unwrap()
            .expect("known");
        assert_eq!(desired.app.as_deref(), Some("netflix"));
        assert_eq!(desired.power, DesiredPower::On);

        let unknown =
            DeviceDescriptor::new(DeviceId::new("tv-9").unwrap(), DeviceOs::Roku, "10.0.0.9");
        assert!(provider.desired_state(&unknown).await.unwrap().is_none());

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }

    #[tokio::test]
    async fn a_missing_file_is_unavailable_not_empty() {
        let provider = StaticFileProvider::new("/nonexistent/tv-rs/desired.json");
        let error = provider
            .desired_state(&device())
            .await
            .expect_err("missing");
        assert!(matches!(error, ProviderError::Unavailable(_)));
    }

    #[test]
    fn malformed_state_is_rejected() {
        let error =
            serde_json::from_str::<HashMap<String, DesiredState>>(r#"{"tv-1":{"power":"maybe"}}"#)
                .expect_err("invalid power");
        assert!(!error.to_string().is_empty());
    }
}
