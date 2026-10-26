//! Desired state pushed over MQTT — the integration most CMS vendors ask for first.
//!
//! The agent subscribes to `<prefix>/+/desired`, where the wildcard is the device id,
//! and keeps the latest payload per device in memory. `desired_state()` then answers
//! synchronously from that cache, so a reconcile pass never waits on the broker.
//!
//! A payload is the same JSON shape as every other provider:
//!
//! ```json
//! { "power": "on", "app": "netflix" }
//! ```
//!
//! Retained messages are the intended pattern: a device that joins late, or an agent
//! that restarts, gets the current state without waiting for the CMS to publish again.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS, Transport};
use tokio::task::JoinHandle;
use tv_rs_core::DeviceDescriptor;

use crate::desired::{DesiredState, DesiredStateProvider, ProviderError};

/// TLS configuration parameters for MQTT connection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MqttTlsConfig {
    /// Path to custom CA certificate file.
    pub ca_cert: Option<PathBuf>,
    /// Path to client certificate file (for mTLS).
    pub client_cert: Option<PathBuf>,
    /// Path to client key file (for mTLS).
    pub client_key: Option<PathBuf>,
}

impl MqttTlsConfig {
    /// Create a new `MqttTlsConfig` with default options.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the custom CA certificate path.
    #[must_use]
    pub fn with_ca_cert(mut self, path: impl Into<PathBuf>) -> Self {
        self.ca_cert = Some(path.into());
        self
    }

    /// Set the client certificate path for mTLS.
    #[must_use]
    pub fn with_client_cert(mut self, path: impl Into<PathBuf>) -> Self {
        self.client_cert = Some(path.into());
        self
    }

    /// Set the client key path for mTLS.
    #[must_use]
    pub fn with_client_key(mut self, path: impl Into<PathBuf>) -> Self {
        self.client_key = Some(path.into());
        self
    }
}

/// Transport mode for the MQTT connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MqttTransport {
    /// TLS connection (default).
    Tls(MqttTlsConfig),
    /// Plain TCP connection (for trusted local segments).
    Tcp,
}

impl Default for MqttTransport {
    fn default() -> Self {
        Self::Tls(MqttTlsConfig::default())
    }
}

/// Desired state fed by an MQTT subscription.
pub struct MqttSubscribeProvider {
    topic_prefix: String,
    latest: Arc<Mutex<HashMap<String, DesiredState>>>,
    task: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for MqttSubscribeProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let known = self.latest.lock().map(|map| map.len()).unwrap_or(0);
        f.debug_struct("MqttSubscribeProvider")
            .field("topic_prefix", &self.topic_prefix)
            .field("devices_known", &known)
            .field("subscribed", &self.task.is_some())
            .finish()
    }
}

impl Drop for MqttSubscribeProvider {
    fn drop(&mut self) {
        // Dropping the provider stops the subscription; the broker connection is
        // owned by that task.
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl MqttSubscribeProvider {
    /// Connects to `broker` (`host:port`) using default TLS settings and subscribes to `<prefix>/+/desired`.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Unavailable`] when the broker address cannot be
    /// parsed or the subscription is rejected.
    pub async fn connect(
        broker: &str,
        topic_prefix: &str,
        client_id: &str,
    ) -> Result<Self, ProviderError> {
        Self::connect_with_transport(broker, topic_prefix, client_id, MqttTransport::default())
            .await
    }

    /// Connects to `broker` (`host:port`) using the specified `transport` and subscribes to `<prefix>/+/desired`.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Unavailable`] when the broker address cannot be
    /// parsed, transport certificates cannot be loaded, or the subscription is rejected.
    pub async fn connect_with_transport(
        broker: &str,
        topic_prefix: &str,
        client_id: &str,
        transport: MqttTransport,
    ) -> Result<Self, ProviderError> {
        let (host, port) = broker.rsplit_once(':').ok_or_else(|| {
            ProviderError::Unavailable(format!("broker must be host:port, got '{broker}'"))
        })?;
        let port: u16 = port
            .parse()
            .map_err(|_| ProviderError::Unavailable(format!("'{port}' is not a port")))?;

        let mut options = MqttOptions::new(client_id, host, port);

        match transport {
            MqttTransport::Tcp => {
                options.set_transport(Transport::tcp());
            }
            MqttTransport::Tls(tls_config) => {
                let ca = if let Some(ca_path) = &tls_config.ca_cert {
                    std::fs::read(ca_path).map_err(|e| {
                        ProviderError::Unavailable(format!(
                            "failed to read CA cert at '{}': {e}",
                            ca_path.display()
                        ))
                    })?
                } else {
                    Vec::new()
                };

                let client_auth = match (tls_config.client_cert, tls_config.client_key) {
                    (Some(cert_path), Some(key_path)) => {
                        let cert_bytes = std::fs::read(&cert_path).map_err(|e| {
                            ProviderError::Unavailable(format!(
                                "failed to read client cert at '{}': {e}",
                                cert_path.display()
                            ))
                        })?;
                        let key_bytes = std::fs::read(&key_path).map_err(|e| {
                            ProviderError::Unavailable(format!(
                                "failed to read client key at '{}': {e}",
                                key_path.display()
                            ))
                        })?;
                        Some((cert_bytes, key_bytes))
                    }
                    (None, None) => None,
                    _ => {
                        return Err(ProviderError::Unavailable(
                            "both client cert and client key must be provided for mTLS".to_string(),
                        ));
                    }
                };

                options.set_transport(Transport::tls(ca, client_auth, None));
            }
        }

        let (client, mut eventloop) = AsyncClient::new(options, 32);

        let topic = format!("{}/+/desired", topic_prefix.trim_end_matches('/'));
        client
            .subscribe(topic.clone(), QoS::AtLeastOnce)
            .await
            .map_err(|error| ProviderError::Unavailable(error.to_string()))?;

        let latest: Arc<Mutex<HashMap<String, DesiredState>>> = Arc::default();
        let store = Arc::clone(&latest);
        let prefix = topic_prefix.trim_end_matches('/').to_string();
        let task_prefix = prefix.clone();

        let task = tokio::spawn(async move {
            loop {
                match eventloop.poll().await {
                    Ok(Event::Incoming(Packet::Publish(publish))) => {
                        let topic = publish.topic.clone();
                        match decode_publish(&task_prefix, &topic, &publish.payload) {
                            Some((device, desired)) => {
                                if let Ok(mut map) = store.lock() {
                                    map.insert(device, desired);
                                }
                            }
                            // A malformed payload is skipped, not fatal: one bad
                            // publisher must not take the subscription down.
                            None => continue,
                        }
                    }
                    Ok(_) => {}
                    // The event loop reconnects internally; a poll error is normal
                    // when the broker restarts.
                    Err(_) => continue,
                }
            }
        });

        Ok(Self {
            topic_prefix: prefix,
            latest,
            task: Some(task),
        })
    }

    /// The topic prefix this provider follows.
    #[must_use]
    pub fn topic_prefix(&self) -> &str {
        &self.topic_prefix
    }

    /// Devices currently known from published messages.
    #[must_use]
    pub fn known_devices(&self) -> Vec<String> {
        self.latest
            .lock()
            .map(|map| map.keys().cloned().collect())
            .unwrap_or_default()
    }
}

#[async_trait]
impl DesiredStateProvider for MqttSubscribeProvider {
    async fn desired_state(
        &self,
        device: &DeviceDescriptor,
    ) -> Result<Option<DesiredState>, ProviderError> {
        Ok(self
            .latest
            .lock()
            .map_err(|_| ProviderError::Unavailable("desired-state cache poisoned".to_string()))?
            .get(device.id.as_str())
            .cloned())
    }
}

/// Decodes one publish into `(device id, desired state)`, or `None` when it is not
/// for this provider or the payload is unusable.
fn decode_publish(prefix: &str, topic: &str, payload: &[u8]) -> Option<(String, DesiredState)> {
    let rest = topic.strip_prefix(prefix)?.strip_prefix('/')?;
    let device = rest.strip_suffix("/desired")?;
    if device.is_empty() || device.contains('/') {
        return None;
    }
    let desired: DesiredState = serde_json::from_slice(payload).ok()?;
    Some((device.to_string(), desired))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tv_rs_core::{DeviceId, DeviceOs};

    #[test]
    fn a_publish_is_decoded_into_device_and_state() {
        let (device, desired) = decode_publish(
            "screens",
            "screens/tv-1/desired",
            br#"{"power":"on","app":"netflix"}"#,
        )
        .expect("decodes");
        assert_eq!(device, "tv-1");
        assert_eq!(desired.app.as_deref(), Some("netflix"));
    }

    #[test]
    fn foreign_topics_and_bad_payloads_are_ignored() {
        assert!(decode_publish("screens", "other/tv-1/desired", b"{}").is_none());
        assert!(decode_publish("screens", "screens/tv-1/state", b"{}").is_none());
        assert!(decode_publish("screens", "screens/tv-1/desired", b"not json").is_none());
        assert!(decode_publish("screens", "screens//desired", b"{}").is_none());
        assert!(decode_publish("screens", "screens/a/b/desired", b"{}").is_none());
    }

    #[tokio::test]
    async fn a_provider_with_no_messages_has_no_opinion() {
        // Constructed without a broker: the cache is the contract under test, and it
        // starts empty — "no opinion" is not "turn it off".
        let provider = MqttSubscribeProvider {
            topic_prefix: "screens".to_string(),
            latest: Arc::default(),
            task: None,
        };
        let device =
            DeviceDescriptor::new(DeviceId::new("tv-1").unwrap(), DeviceOs::Roku, "10.0.0.5");
        assert!(provider.desired_state(&device).await.unwrap().is_none());
        assert!(provider.known_devices().is_empty());
    }

    #[test]
    fn mqtt_tls_config_builder() {
        let config = MqttTlsConfig::new()
            .with_ca_cert("/path/to/ca.pem")
            .with_client_cert("/path/to/client.pem")
            .with_client_key("/path/to/client.key");

        assert_eq!(config.ca_cert, Some(PathBuf::from("/path/to/ca.pem")));
        assert_eq!(
            config.client_cert,
            Some(PathBuf::from("/path/to/client.pem"))
        );
        assert_eq!(config.client_key, Some(PathBuf::from("/path/to/client.key")));
    }

    #[tokio::test]
    async fn connect_rejects_invalid_broker_format() {
        let err = MqttSubscribeProvider::connect("invalid_broker", "screens", "client")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("broker must be host:port"));

        let err2 = MqttSubscribeProvider::connect("host:not_a_port", "screens", "client")
            .await
            .unwrap_err();
        assert!(err2.to_string().contains("is not a port"));
    }

    #[tokio::test]
    async fn connect_with_transport_validates_mtls_arguments() {
        let config = MqttTlsConfig::new().with_client_cert("/path/to/cert.pem");
        let err = MqttSubscribeProvider::connect_with_transport(
            "localhost:1883",
            "screens",
            "client",
            MqttTransport::Tls(config),
        )
        .await
        .unwrap_err();

        assert!(err
            .to_string()
            .contains("both client cert and client key must be provided for mTLS"));
    }

    #[tokio::test]
    async fn connect_with_transport_handles_missing_files() {
        let config = MqttTlsConfig::new().with_ca_cert("/nonexistent/ca.pem");
        let err = MqttSubscribeProvider::connect_with_transport(
            "localhost:1883",
            "screens",
            "client",
            MqttTransport::Tls(config),
        )
        .await
        .unwrap_err();

        assert!(err.to_string().contains("failed to read CA cert"));
    }
}
