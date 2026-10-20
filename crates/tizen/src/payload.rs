//! Serde payload structs exchanged with the Tizen TV.
//!
//! Naming follows `<Protocol><Payload>` from `docs/00-rust-package-standard.md` §6.

use serde::{Deserialize, Serialize};

/// The `ms.channel.connect` request sent to open the remote channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TizenChannelConnectPayload {
    /// Always `ms.channel.connect`.
    pub method: String,
    /// Request parameters.
    pub params: TizenChannelConnectParams,
}

/// Parameters of a [`TizenChannelConnectPayload`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TizenChannelConnectParams {
    /// A token previously issued by the TV, if any. Omitted when absent, which
    /// triggers the on-screen PIN prompt.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Client identification data.
    pub data: TizenChannelConnectData,
}

/// Client identification data for the connect request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TizenChannelConnectData {
    /// Human-readable remote name (base64-encoded in the URL query string).
    pub name: String,
    /// Application identifier, sent as the `appId` JSON key.
    #[serde(rename = "appId")]
    pub app_id: String,
}

/// An event frame received from the TV after connecting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TizenChannelConnectResponse {
    /// Event name, e.g. `ms.channel.connect` or `ms.channel.unauthorized`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// Event payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<TizenChannelConnectResponseData>,
    /// Message id echoed by the TV.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Result object (unused in practice).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
}

/// Payload of a [`TizenChannelConnectResponse`] event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TizenChannelConnectResponseData {
    /// Pairing token issued by the TV; persisted by the application for reconnects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// TV name reported by some firmware revisions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// TV name reported under the `deviceName` spelling by other revisions.
    #[serde(
        rename = "deviceName",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub device_name: Option<String>,
    /// Connected-client list. Its shape varies by firmware, so it is kept opaque.
    #[serde(default)]
    pub clients: Option<serde_json::Value>,
}

/// The `ms.channel.emit` request used to launch an application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TizenChannelEmitPayload {
    /// Always `ms.channel.emit`.
    pub method: String,
    /// Emit parameters.
    pub params: TizenChannelEmitParams,
}

/// Parameters of a [`TizenChannelEmitPayload`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TizenChannelEmitParams {
    /// Event name; `ed.apps.launch` for app launches.
    pub event: String,
    /// Event target; `host` for the TV.
    pub to: String,
    /// Event payload.
    pub data: TizenChannelLaunchData,
}

/// Data carried by the `ed.apps.launch` event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TizenChannelLaunchData {
    /// Samsung application id.
    #[serde(rename = "appId")]
    pub app_id: String,
    /// `NATIVE_LAUNCH` or `DEEP_LINK`.
    pub action_type: String,
    /// Optional deep-link argument.
    #[serde(rename = "metaTag", default, skip_serializing_if = "Option::is_none")]
    pub meta_tag: Option<String>,
}

/// Legacy `ms.channel.launch` event used by older community clients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TizenLegacyLaunchPayload {
    /// Always `ms.channel.launch`.
    pub event: String,
    /// Numeric application id.
    pub data: String,
    /// Message id.
    pub id: u32,
}

/// The `ms.remote.control` request used to press a remote key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TizenRemoteCommand {
    /// Always `ms.remote.control`.
    pub method: String,
    /// Command parameters.
    pub params: TizenRemoteCommandParams,
}

/// Parameters of a [`TizenRemoteCommand`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TizenRemoteCommandParams {
    /// Command type, e.g. `Click`.
    #[serde(rename = "Cmd")]
    pub cmd: String,
    /// Key code, e.g. `KEY_HOME`.
    #[serde(rename = "DataOfCmd")]
    pub data_of_cmd: String,
    /// Stringified boolean, e.g. `false`.
    #[serde(rename = "Option")]
    pub option: String,
    /// Remote type, e.g. `SendRemoteKey`.
    #[serde(rename = "TypeOfRemote")]
    pub type_of_remote: String,
}

/// Result of a DIAL app launch.
///
/// DIAL launching is fire-and-forget: the TV acknowledges the request with an
/// empty `200`/`201` body, so this only records whatever raw body was returned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TizenDialLaunchResponse {
    /// Raw response body (normally empty).
    pub raw: String,
}
