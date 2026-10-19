//! Desired state fetched over HTTP, for the simplest CMS integration.
//!
//! The contract matches [`StaticFileProvider`](crate::StaticFileProvider): the
//! endpoint returns a JSON object mapping device id to desired state. Nothing about
//! how that object is produced belongs here — a CMS can render it from a database, a
//! static file behind nginx, or a lambda.
//!
//! ```no_run
//! use tv_rs_agent::HttpPollProvider;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let provider = HttpPollProvider::new("http://10.0.0.2:8080/desired.json")?;
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use tv_rs_core::DeviceDescriptor;

use crate::desired::{DesiredState, DesiredStateProvider, ProviderError};

/// Reads desired state from an HTTP endpoint on every call.
#[derive(Debug, Clone)]
pub struct HttpPollProvider {
    url: String,
    http: reqwest::Client,
}

impl HttpPollProvider {
    /// Builds a provider with a 5 s timeout.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Unavailable`] if the HTTP client cannot be built.
    pub fn new(url: impl Into<String>) -> Result<Self, ProviderError> {
        Self::with_timeout(url, Duration::from_secs(5))
    }

    /// Builds a provider with an explicit request timeout.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Unavailable`] if the HTTP client cannot be built.
    pub fn with_timeout(url: impl Into<String>, timeout: Duration) -> Result<Self, ProviderError> {
        let http = reqwest::Client::builder()
            .timeout(timeout)
            .connect_timeout(Duration::from_secs(3))
            .build()
            .map_err(|error| ProviderError::Unavailable(error.to_string()))?;
        Ok(Self {
            url: url.into(),
            http,
        })
    }

    /// The endpoint being polled.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Fetches the whole manifest.
    ///
    /// Exposed because a caller may want to validate the manifest once at start-up
    /// and fail loudly, rather than discovering a typo per device.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Unavailable`] on transport failure and
    /// [`ProviderError::Invalid`] when the body is not the expected map.
    pub async fn fetch(&self) -> Result<HashMap<String, DesiredState>, ProviderError> {
        let response = self
            .http
            .get(&self.url)
            .send()
            .await
            .map_err(|error| ProviderError::Unavailable(error.to_string()))?;

        if !response.status().is_success() {
            return Err(ProviderError::Unavailable(format!(
                "{} returned {}",
                self.url,
                response.status()
            )));
        }

        let text = response
            .text()
            .await
            .map_err(|error| ProviderError::Unavailable(error.to_string()))?;
        serde_json::from_str(&text).map_err(|error| ProviderError::Invalid(error.to_string()))
    }
}

#[async_trait]
impl DesiredStateProvider for HttpPollProvider {
    async fn desired_state(
        &self,
        device: &DeviceDescriptor,
    ) -> Result<Option<DesiredState>, ProviderError> {
        Ok(self.fetch().await?.get(device.id.as_str()).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tv_rs_core::{DeviceId, DeviceOs};
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn device() -> DeviceDescriptor {
        DeviceDescriptor::new(DeviceId::new("tv-1").unwrap(), DeviceOs::Roku, "10.0.0.5")
    }

    #[tokio::test]
    async fn a_manifest_answers_per_device() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/desired.json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(r#"{"tv-1":{"power":"on","app":"netflix"}}"#),
            )
            .mount(&server)
            .await;

        let provider = HttpPollProvider::new(format!("{}/desired.json", server.uri())).unwrap();
        let desired = provider
            .desired_state(&device())
            .await
            .unwrap()
            .expect("known");
        assert_eq!(desired.app.as_deref(), Some("netflix"));
    }

    #[tokio::test]
    async fn an_unknown_device_is_none_not_an_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/desired.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"tv-9":{"power":"on"}}"#))
            .mount(&server)
            .await;

        let provider = HttpPollProvider::new(format!("{}/desired.json", server.uri())).unwrap();
        assert!(provider.desired_state(&device()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn a_broken_manifest_is_invalid_not_empty() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/desired.json"))
            .respond_with(ResponseTemplate::new(200).set_body_string("<html>nope</html>"))
            .mount(&server)
            .await;

        let provider = HttpPollProvider::new(format!("{}/desired.json", server.uri())).unwrap();
        match provider.desired_state(&device()).await {
            Err(ProviderError::Invalid(_)) => {}
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn a_server_error_is_unavailable() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let provider = HttpPollProvider::new(server.uri()).unwrap();
        match provider.desired_state(&device()).await {
            Err(ProviderError::Unavailable(message)) => {
                assert!(message.contains("503"), "{message}")
            }
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }
}
