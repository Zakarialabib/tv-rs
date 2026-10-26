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
//! Connections use TLS by default ([`MqttSubscribeProvider::connect`] or
//! [`MqttSubscribeProvider::connect_tls`]). Custom TLS options (such as custom CAs
//! or client certificate authentication) or unencrypted plain TCP connections for
//! trusted local networks can be specified via [`MqttSubscribeProvider::connect_with_transport`].
//!
//! Retained messages are the intended pattern: a device that joins late, or an agent
//! that restarts, gets the current state without waiting for the CMS to publish again.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS, TlsConfiguration, Transport};
use tokio::task::JoinHandle;
use tv_rs_core::DeviceDescriptor;

use crate::desired::{DesiredState, DesiredStateProvider, ProviderError};

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

/// TLS configuration options for MQTT connections.
#[derive(Debug, Clone, Default)]
pub struct MqttTlsConfig {
    /// Custom Root CA certificate(s) in PEM or DER format.
    pub ca_certs: Vec<Vec<u8>>,
    /// Client authentication credentials (cert and private key in PEM/DER).
    pub client_auth: Option<(Vec<u8>, Vec<u8>)>,
}

/// Transport mode for connecting to an MQTT broker.
#[derive(Debug, Clone)]
pub enum MqttTransport {
    /// Unencrypted plain TCP connection.
    Tcp,
    /// Encrypted TLS connection with configurable trust store and optional mTLS.
    Tls(MqttTlsConfig),
}

impl MqttSubscribeProvider {
    /// Connects to `broker` (`host:port`) with plain TCP or TLS according to `transport`,
    /// and subscribes to `<prefix>/+/desired`.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Unavailable`] when the broker address cannot be
    /// parsed or the subscription is rejected.
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
                options.set_transport(Transport::Tcp);
            }
            MqttTransport::Tls(tls_config) => {
                let client_auth = tls_config.client_auth;
                let tls_opts = if tls_config.ca_certs.is_empty() {
                    TlsConfiguration::Simple {
                        ca: Vec::new(),
                        alpn: None,
                        client_auth,
                    }
                } else {
                    TlsConfiguration::Simple {
                        ca: tls_config.ca_certs.into_iter().flatten().collect(),
                        alpn: None,
                        client_auth,
                    }
                };
                options.set_transport(Transport::Tls(tls_opts));
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

    /// Connects to `broker` (`host:port`) using default TLS settings and subscribes to `<prefix>/+/desired`.
    ///
    /// # Errors
    ///
    /// Returns [`ProviderError::Unavailable`] when the broker address cannot be
    /// parsed or the subscription is rejected.
    pub async fn connect_tls(
        broker: &str,
        topic_prefix: &str,
        client_id: &str,
    ) -> Result<Self, ProviderError> {
        Self::connect_with_transport(
            broker,
            topic_prefix,
            client_id,
            MqttTransport::Tls(MqttTlsConfig::default()),
        )
        .await
    }

    /// Connects to `broker` (`host:port`) and subscribes to `<prefix>/+/desired`.
    ///
    /// By default, this uses standard TLS verification. For unencrypted connections on trusted networks,
    /// see [`connect_with_transport`](Self::connect_with_transport) with [`MqttTransport::Tcp`].
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
        Self::connect_tls(broker, topic_prefix, client_id).await
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
    fn transport_and_tls_config_data_structures() {
        let default_cfg = MqttTlsConfig::default();
        assert!(default_cfg.ca_certs.is_empty());
        assert!(default_cfg.client_auth.is_none());

        let custom_cfg = MqttTlsConfig {
            ca_certs: vec![b"fake ca cert".to_vec()],
            client_auth: Some((b"client cert".to_vec(), b"client key".to_vec())),
        };

        let transport_tls = MqttTransport::Tls(custom_cfg.clone());
        let transport_tcp = MqttTransport::Tcp;

        match transport_tls {
            MqttTransport::Tls(cfg) => {
                assert_eq!(cfg.ca_certs.len(), 1);
                assert!(cfg.client_auth.is_some());
            }
            _ => panic!("Expected Tls transport"),
        }

        match transport_tcp {
            MqttTransport::Tcp => {}
            _ => panic!("Expected Tcp transport"),
        }
    }

    #[tokio::test]
    async fn connect_validates_broker_address() {
        let err = MqttSubscribeProvider::connect("invalid_address", "screens", "client_1")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("broker must be host:port"));

        let err_port =
            MqttSubscribeProvider::connect("localhost:not_a_port", "screens", "client_1")
                .await
                .unwrap_err();
        assert!(err_port.to_string().contains("is not a port"));
    }
}
