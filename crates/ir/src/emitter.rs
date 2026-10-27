//! IR Emitter hardware abstraction and implementations.

use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use tracing::info;

use crate::error::IrError;
use crate::protocol::IrSignal;

/// Trait implemented by IR blasters and transmitters (LIRC, network bridge, mock).
#[async_trait]
pub trait IrEmitter: Send + Sync {
    /// Transmits an encoded [`IrSignal`].
    async fn emit(&self, signal: &IrSignal) -> Result<(), IrError>;
}

/// In-memory mock IR emitter for tests and dry-run sweeps.
#[derive(Debug, Clone, Default)]
pub struct MockIrEmitter {
    history: Arc<Mutex<Vec<IrSignal>>>,
}

impl MockIrEmitter {
    /// Builds a new empty mock emitter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a snapshot of all emitted signals.
    #[must_use]
    pub fn emitted_signals(&self) -> Vec<IrSignal> {
        self.history.lock().unwrap().clone()
    }

    /// Returns the last emitted signal, if any.
    #[must_use]
    pub fn last_emitted(&self) -> Option<IrSignal> {
        self.history.lock().unwrap().last().cloned()
    }

    /// Clears the history log.
    pub fn clear(&self) {
        self.history.lock().unwrap().clear();
    }
}

#[async_trait]
impl IrEmitter for MockIrEmitter {
    async fn emit(&self, signal: &IrSignal) -> Result<(), IrError> {
        info!(
            protocol = %signal.protocol,
            carrier_khz = signal.carrier_khz,
            address = format_args!("{:#06x}", signal.address),
            command = format_args!("{:#04x}", signal.command),
            "MockIrEmitter emitting signal"
        );
        self.history.lock().unwrap().push(signal.clone());
        Ok(())
    }
}

/// LIRC (Linux Infrared Remote Control) hardware emitter using `ir-ctl` process execution.
#[derive(Debug, Clone)]
pub struct LircEmitter {
    device_path: String,
}

impl LircEmitter {
    /// Creates a LIRC emitter for a device node (e.g. `/dev/lirc0`).
    #[must_use]
    pub fn new(device_path: impl Into<String>) -> Self {
        Self {
            device_path: device_path.into(),
        }
    }

    /// Default LIRC device path (`/dev/lirc0`).
    #[must_use]
    pub fn default_device() -> Self {
        Self::new("/dev/lirc0")
    }
}

#[async_trait]
impl IrEmitter for LircEmitter {
    async fn emit(&self, signal: &IrSignal) -> Result<(), IrError> {
        let raw_timing = signal.to_raw_timing_string();
        info!(
            device = %self.device_path,
            protocol = %signal.protocol,
            carrier = signal.carrier_khz,
            "LircEmitter transmitting raw IR timing"
        );

        let output = tokio::process::Command::new("ir-ctl")
            .arg("-d")
            .arg(&self.device_path)
            .arg("-c")
            .arg(signal.carrier_khz.to_string())
            .arg("-s")
            .arg(&raw_timing)
            .output()
            .await
            .map_err(|e| IrError::Emitter(format!("failed to execute ir-ctl: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(IrError::Emitter(format!(
                "ir-ctl exited with status {}: {}",
                output.status, stderr
            )));
        }

        Ok(())
    }
}

/// HTTP Network Bridge IR Emitter (for ESP8266/ESP32 network IR blasters).
#[derive(Debug, Clone)]
pub struct NetworkBridgeEmitter {
    endpoint_url: String,
}

impl NetworkBridgeEmitter {
    /// Creates a network bridge emitter targeting an HTTP endpoint URL.
    #[must_use]
    pub fn new(endpoint_url: impl Into<String>) -> Self {
        Self {
            endpoint_url: endpoint_url.into(),
        }
    }
}

#[async_trait]
impl IrEmitter for NetworkBridgeEmitter {
    async fn emit(&self, signal: &IrSignal) -> Result<(), IrError> {
        #[cfg(feature = "network-bridge")]
        {
            let client = reqwest::Client::new();
            let payload = serde_json::json!({
                "protocol": signal.protocol.as_str(),
                "carrier_khz": signal.carrier_khz,
                "address": signal.address,
                "command": signal.command,
                "pulses": signal.pulses,
            });

            let res = client
                .post(&self.endpoint_url)
                .json(&payload)
                .send()
                .await
                .map_err(|e| IrError::Emitter(format!("http request to ir bridge failed: {e}")))?;

            if !res.status().is_success() {
                return Err(IrError::Emitter(format!(
                    "ir bridge returned HTTP error status: {}",
                    res.status()
                )));
            }
            Ok(())
        }

        #[cfg(not(feature = "network-bridge"))]
        {
            info!(
                endpoint = %self.endpoint_url,
                protocol = %signal.protocol,
                "NetworkBridgeEmitter simulated transmission (network-bridge feature disabled)"
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{IrProtocol, IrSignal};

    #[tokio::test]
    async fn mock_emitter_records_signals() {
        let emitter = MockIrEmitter::new();
        let signal = IrSignal::encode(IrProtocol::Nec, 0x07, 0x02, None).unwrap();
        emitter.emit(&signal).await.unwrap();

        assert_eq!(emitter.emitted_signals().len(), 1);
        assert_eq!(emitter.last_emitted().unwrap(), signal);
    }
}
