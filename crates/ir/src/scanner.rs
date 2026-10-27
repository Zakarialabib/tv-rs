//! `UniversalScanner` with feedback-loop brand detection logic.

use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};
use tv_rs_core::Key;

use crate::emitter::IrEmitter;
use crate::error::IrError;
use crate::profile::{all_default_profiles, IrProfile};

/// Trait for verifying whether an emitted IR signal produced a response from the device.
#[async_trait]
pub trait IrScanFeedback: Send + Sync {
    /// Returns `Ok(true)` if the feedback probe confirms the TV/box woke or responded.
    async fn is_active(&self) -> Result<bool, IrError>;
}

/// HDMI-CEC feedback loop checking device activity over the HDMI CEC bus.
pub struct CecFeedback<T: tv_rs_cec::CecTransport> {
    client: tv_rs_cec::CecClient<T>,
    target_address: tv_rs_cec::CecAddress,
}

impl<T: tv_rs_cec::CecTransport> CecFeedback<T> {
    /// Creates a CEC feedback probe using the given transport and target address.
    #[must_use]
    pub fn new(transport: T, target_address: tv_rs_cec::CecAddress) -> Self {
        Self {
            client: tv_rs_cec::CecClient::new(transport),
            target_address,
        }
    }
}

#[async_trait]
impl<T: tv_rs_cec::CecTransport> IrScanFeedback for CecFeedback<T> {
    async fn is_active(&self) -> Result<bool, IrError> {
        // Query target device over CEC bus
        match self.client.power_on(self.target_address).await {
            Ok(_) => Ok(true),
            Err(e) => {
                warn!(error = %e, "CEC feedback probe returned error");
                Ok(false)
            }
        }
    }
}

/// TCP Power Probe feedback loop checking network service ports.
#[derive(Debug, Clone)]
pub struct TcpProbeFeedback {
    target_ip: String,
    ports: Vec<u16>,
    connect_timeout: Duration,
}

impl TcpProbeFeedback {
    /// Creates a TCP probe feedback checker targeting `target_ip`.
    #[must_use]
    pub fn new(target_ip: impl Into<String>) -> Self {
        Self {
            target_ip: target_ip.into(),
            ports: vec![8001, 8002, 3000, 3001, 8060, 1515],
            connect_timeout: Duration::from_millis(500),
        }
    }

    /// Overrides the target ports to probe.
    #[must_use]
    pub fn with_ports(mut self, ports: Vec<u16>) -> Self {
        self.ports = ports;
        self
    }
}

#[async_trait]
impl IrScanFeedback for TcpProbeFeedback {
    async fn is_active(&self) -> Result<bool, IrError> {
        for port in &self.ports {
            let addr = format!("{}:{}", self.target_ip, port);
            if let Ok(Ok(_)) =
                tokio::time::timeout(self.connect_timeout, tokio::net::TcpStream::connect(&addr))
                    .await
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

/// In-memory mock feedback loop for tests.
#[derive(Debug, Clone)]
pub struct MockFeedback {
    responses: Arc<std::sync::Mutex<Vec<bool>>>,
}

impl MockFeedback {
    /// Creates a mock feedback loop returning sequence of booleans.
    #[must_use]
    pub fn new(sequence: Vec<bool>) -> Self {
        Self {
            responses: Arc::new(std::sync::Mutex::new(sequence)),
        }
    }
}

#[async_trait]
impl IrScanFeedback for MockFeedback {
    async fn is_active(&self) -> Result<bool, IrError> {
        let mut guard = self.responses.lock().unwrap();
        if guard.is_empty() {
            Ok(false)
        } else {
            Ok(guard.remove(0))
        }
    }
}

/// Universal Scanner iterating through candidate IR code profiles and verifying feedback.
pub struct UniversalScanner<E: IrEmitter> {
    emitter: Arc<E>,
    candidate_profiles: Vec<IrProfile>,
    verification_delay: Duration,
}

impl<E: IrEmitter> UniversalScanner<E> {
    /// Creates a new scanner using `emitter` and standard brand profiles.
    #[must_use]
    pub fn new(emitter: Arc<E>) -> Self {
        Self {
            emitter,
            candidate_profiles: all_default_profiles(),
            verification_delay: Duration::from_millis(100),
        }
    }

    /// Sets custom candidate profiles ladder.
    #[must_use]
    pub fn with_profiles(mut self, profiles: Vec<IrProfile>) -> Self {
        self.candidate_profiles = profiles;
        self
    }

    /// Sets delay duration between IR signal emission and feedback verification check.
    #[must_use]
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.verification_delay = delay;
        self
    }

    /// Executes the detection loop ladder across candidate profiles.
    ///
    /// # Errors
    ///
    /// Returns [`IrError::ScanFailed`] if an emitter or feedback error occurs.
    pub async fn scan<F: IrScanFeedback>(
        &self,
        feedback: &F,
    ) -> Result<Option<IrProfile>, IrError> {
        info!(
            candidates_count = self.candidate_profiles.len(),
            "Starting Universal IR brand detection scanner sweep"
        );

        for profile in &self.candidate_profiles {
            info!(
                brand = %profile.brand,
                profile = %profile.name,
                protocol = %profile.protocol,
                "Testing IR candidate profile in ladder"
            );

            let power_sig = match profile.signal_for_key(Key::Power) {
                Ok(sig) => sig,
                Err(e) => {
                    warn!(profile = %profile.name, error = %e, "Skipping profile without valid Power code");
                    continue;
                }
            };

            // Emit power toggle code
            self.emitter.emit(&power_sig).await?;

            // Wait verification delay before checking feedback probe
            if !self.verification_delay.is_zero() {
                tokio::time::sleep(self.verification_delay).await;
            }

            // Perform feedback probe
            if feedback.is_active().await? {
                info!(
                    brand = %profile.brand,
                    profile = %profile.name,
                    "Successfully detected working IR brand profile!"
                );
                return Ok(Some(profile.clone()));
            }
        }

        info!("Universal IR scanner finished sweep without finding matching profile");
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emitter::MockIrEmitter;

    #[tokio::test]
    async fn scanner_detects_winning_profile_in_ladder() {
        let emitter = Arc::new(MockIrEmitter::new());

        // Mock feedback: false for Samsung (first), true for LG (second)
        let feedback = MockFeedback::new(vec![false, true]);

        let scanner = UniversalScanner::new(emitter.clone()).with_delay(Duration::from_millis(1));
        let matched = scanner.scan(&feedback).await.unwrap();

        assert!(matched.is_some());
        let winning_profile = matched.unwrap();
        assert_eq!(winning_profile.brand, "lg");

        assert_eq!(emitter.emitted_signals().len(), 2);
    }
}
