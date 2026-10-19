//! Serde payload structs for the LG webOS SSAP JSON-RPC framing.
//!
//! Naming follows the workspace convention `<Protocol><Payload>`.

use serde::{Deserialize, Serialize};

use crate::types::WEBOS_DEFAULT_PERMISSIONS;

/// The manifest advertised during SSAP registration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SsapManifest {
    /// SSAP manifest schema version. Always `1` for this client.
    #[serde(rename = "manifestVersion")]
    pub manifest_version: u32,
    /// Permissions the client requests from the TV.
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Optional signature block; `None` serializes to JSON `null`.
    #[serde(default)]
    pub signed: Option<serde_json::Value>,
}

impl SsapManifest {
    /// Builds the default manifest from [`WEBOS_DEFAULT_PERMISSIONS`].
    #[must_use]
    pub fn control() -> Self {
        Self {
            manifest_version: 1,
            permissions: WEBOS_DEFAULT_PERMISSIONS
                .iter()
                .map(|permission| (*permission).to_string())
                .collect(),
            signed: None,
        }
    }
}

/// Payload of an SSAP `register` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SsapRegisterPayload {
    /// Pairing mode; [`crate::types::WEBOS_PAIRING_TYPE_PROMPT`] triggers the on-screen prompt.
    #[serde(rename = "pairingType")]
    pub pairing_type: String,
    /// The permission manifest to register with.
    pub manifest: SsapManifest,
}

/// Top-level SSAP `register` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SsapRegisterRequest {
    /// Correlation id echoed back by the TV.
    pub id: String,
    /// Frame type; always `"register"` for this struct.
    #[serde(rename = "type")]
    pub kind: String,
    /// Registration payload.
    pub payload: SsapRegisterPayload,
}

/// Payload of the TV's response to a `register` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SsapRegisterResponsePayload {
    /// The long-lived key issued by the TV, to be reused on later connections.
    #[serde(rename = "client-key")]
    pub client_key: Option<String>,
}

/// Top-level response to an SSAP `register` request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SsapRegisterResponse {
    /// Correlation id, when the TV echoes it.
    pub id: Option<String>,
    /// Frame type; `"registered"` on success or `"error"` on rejection.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Registration payload carrying the issued `client-key`.
    pub payload: Option<SsapRegisterResponsePayload>,
    /// Error body when the TV rejects registration.
    pub error: Option<SsapErrorBody>,
}

/// A JSON-RPC-ish SSAP command request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SsapRequest {
    /// Correlation id used to match the response.
    pub id: String,
    /// Frame type; always `"request"` for this struct.
    #[serde(rename = "type")]
    pub kind: String,
    /// Target SSAP URI, e.g. `ssap://audio/setVolume`.
    pub uri: String,
    /// Optional command payload; omitted from the wire form when `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
}

/// The TV's reply to an SSAP command request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SsapResponse {
    /// Correlation id, when the TV echoes it.
    pub id: Option<String>,
    /// Frame type; `"response"` or `"error"`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Response payload, absent for some commands.
    #[serde(default)]
    pub payload: Option<serde_json::Value>,
    /// Error body when the command failed.
    #[serde(default)]
    pub error: Option<SsapErrorBody>,
}

/// The `error` object embedded in an SSAP frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SsapErrorBody {
    /// Numeric SSAP error code, e.g. `401`.
    pub code: i64,
    /// Human-readable error message.
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::SsapManifest;

    #[test]
    fn control_manifest_has_default_permissions() {
        let manifest = SsapManifest::control();
        assert_eq!(manifest.manifest_version, 1);
        assert!(!manifest.permissions.is_empty());
        assert!(manifest
            .permissions
            .iter()
            .any(|permission| permission == "CONTROL_POWER"));
    }
}
