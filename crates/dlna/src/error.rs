//! Error type for DLNA transport operations.
//!
//! Ported from the Signage `plugin-dlna` crate (adoption U3). The
//! `InvalidPlaylist` variant stayed behind with the playlist orchestrator —
//! playlists are a signage policy, not transport.

use thiserror::Error;

/// Alias for fallible transport calls.
pub type Result<T> = std::result::Result<T, DlnaError>;

/// Every failure mode of the DLNA transport (discovery, control, eventing).
#[derive(Debug, Error)]
pub enum DlnaError {
    /// A requested device (e.g. by UDN) is not present.
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    /// Socket/bind/multicast I/O failure.
    #[error("Network error: {0}")]
    Network(#[from] std::io::Error),

    /// HTTP(S) request failure against the renderer.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// XML document failed to parse.
    #[error("XML parse error: {0}")]
    Xml(String),

    /// XML event stream failed to parse.
    #[error("XML parse error: {0}")]
    ParseError(String),

    /// The device exposes no service of the requested type, so that function is
    /// not available on it.
    ///
    /// Names the service type rather than one specific service: a renderer can
    /// offer AVTransport (castable) without RenderingControl (no volume), and the
    /// caller needs to know which of the two it asked for.
    #[error("Device has no {0} service")]
    ServiceNotFound(String),

    /// The renderer answered a SOAP action with a UPnP fault body.
    #[error("AVTransport SOAP fault: {0}")]
    AvTransportFault(String),

    /// A SOAP control call failed (client build, HTTP status, transport).
    #[error("SOAP control error: {0}")]
    SoapError(String),

    /// The device cannot run this transport (e.g. not a MediaRenderer).
    #[error("Device incompatible: {0}")]
    IncompatibleDevice(String),

    /// The multicast bind/send was blocked (usually a firewall on UDP 1900).
    #[error("Multicast blocked by firewall (allow UDP 1900 / SSDP)")]
    MulticastBlocked,

    /// SSDP discovery answered nothing within the window.
    #[error("SSDP discovery timed out with no responses")]
    NoDevicesFound,
}

impl From<DlnaError> for String {
    fn from(err: DlnaError) -> Self {
        match err {
            DlnaError::MulticastBlocked => {
                "Network firewall may be blocking device discovery. Ensure UDP port 1900 (SSDP) is allowed.".to_string()
            }
            DlnaError::IncompatibleDevice(msg) => {
                format!("This display doesn't support DLNA casting. {msg}")
            }
            DlnaError::NoDevicesFound => {
                "No DLNA/UPnP displays found on the network. Verify the device is powered on and on the same subnet.".to_string()
            }
            _ => err.to_string(),
        }
    }
}
