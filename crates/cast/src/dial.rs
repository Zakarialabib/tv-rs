//! DIAL (Discovery and Launch) HTTP fallback implementation.

use crate::error::{CastError, Result};
use reqwest::Client;

/// Default DIAL port.
pub const DIAL_PORT: u16 = 8008;

/// A simple DIAL HTTP fallback client for launch and stop.
pub struct DialClient {
    client: Client,
    base_url: String,
}

impl DialClient {
    /// Creates a new DIAL client for the given IP address.
    pub fn new(ip: &str) -> Self {
        let base_url = format!("http://{}:{}/apps", ip, DIAL_PORT);
        Self {
            client: Client::new(),
            base_url,
        }
    }

    /// Launches an application by ID.
    pub async fn launch(&self, app_id: &str) -> Result<()> {
        let url = format!("{}/{}", self.base_url, app_id);
        let res = self.client.post(&url).send().await?;
        if res.status().is_success() {
            Ok(())
        } else {
            Err(CastError::Http(res.error_for_status().unwrap_err()))
        }
    }

    /// Stops an application by ID.
    pub async fn stop(&self, app_id: &str) -> Result<()> {
        let url = format!("{}/{}", self.base_url, app_id);
        let res = self.client.delete(&url).send().await?;
        if res.status().is_success() {
            Ok(())
        } else {
            Err(CastError::Http(res.error_for_status().unwrap_err()))
        }
    }

    /// Gets application status.
    pub async fn status(&self, app_id: &str) -> Result<String> {
        let url = format!("{}/{}", self.base_url, app_id);
        let res = self.client.get(&url).send().await?;
        if res.status().is_success() {
            let body = res.text().await?;
            Ok(body)
        } else {
            Err(CastError::Http(res.error_for_status().unwrap_err()))
        }
    }
}
