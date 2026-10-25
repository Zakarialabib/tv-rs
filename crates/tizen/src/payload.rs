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

/// What `GET /api/v2/` reports, narrowed to what this workspace acts on.
///
/// Every field is optional because the firmware decides: `wifiMac` is absent on some
/// models, and the honest answer there is "no address", never a guess.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceInfo {
    /// `device.wifiMac` — the address a Wake-on-LAN packet targets.
    ///
    /// **It is the Wi-Fi address even on a set that is currently wired**, so on a
    /// wired set it may not be the interface that answers a magic packet.
    /// [`DeviceInfo::network_type`] is what tells the two apart.
    pub wifi_mac: Option<String>,
    /// `device.networkType` — `"wireless"` or `"wired"`.
    pub network_type: Option<String>,
    /// `device.modelName`, when reported.
    pub model_name: Option<String>,
}

impl DeviceInfo {
    /// The address to wake, when the set reports one.
    ///
    /// Returns the Wi-Fi address; cross-check [`DeviceInfo::network_type`] before
    /// treating it as the live interface.
    #[must_use]
    pub fn mac(&self) -> Option<String> {
        self.wifi_mac.clone()
    }
}

/// Parses the `/api/v2/` document.
///
/// A document that is not JSON is a JSON error; one that parses but carries no
/// `device` object is [`crate::TizenError::Protocol`], because that is a firmware
/// change rather than an empty answer — and calling it "no MAC" would hide it.
pub fn parse_device_info(body: &str) -> crate::Result<DeviceInfo> {
    let value: serde_json::Value = serde_json::from_str(body)?;
    let device = value.get("device").ok_or_else(|| {
        crate::TizenError::Protocol("device info carried no `device` object".to_owned())
    })?;

    let text = |key: &str| {
        device
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };

    Ok(DeviceInfo {
        wifi_mac: text("wifiMac"),
        network_type: text("networkType"),
        model_name: text("modelName"),
    })
}

#[cfg(test)]
mod device_info_tests {
    use super::{parse_device_info, DeviceInfo};

    /// The documented shape, from a real set: `wifiMac` inside the top-level
    /// `device` object. Trimmed to the fields this workspace reads.
    #[test]
    fn the_documented_document_yields_the_wifi_address() {
        let body = r#"{
            "device": {
                "modelName": "QN55LS03RAFXZA",
                "networkType": "wireless",
                "wifiMac": "B8:BC:5B:B1:F2:D2"
            },
            "uri": "http://192.168.10.143:8001/api/v2/",
            "version": "2.0.25"
        }"#;
        let info = parse_device_info(body).expect("the documented shape parses");
        assert_eq!(info.mac().as_deref(), Some("B8:BC:5B:B1:F2:D2"));
        assert_eq!(info.network_type.as_deref(), Some("wireless"));
        assert_eq!(info.model_name.as_deref(), Some("QN55LS03RAFXZA"));
    }

    /// `wifiMac` is optional on some firmware, and the honest answer is then "no
    /// address" — not a guess, and not an error that hides the rest of the document.
    #[test]
    fn a_document_without_wifi_mac_answers_no_address() {
        let info = parse_device_info(r#"{"device":{"networkType":"wired"}}"#).expect("parses");
        assert_eq!(info.mac(), None);
        assert_eq!(info.network_type.as_deref(), Some("wired"));
    }

    /// The trap the playbook records: `ssid` is sometimes itself MAC-shaped. It is not
    /// a second source, and this pins that it is not read as one.
    #[test]
    fn a_mac_shaped_ssid_is_not_an_address() {
        let body = r#"{"device":{"networkType":"wireless","ssid":"78:45:58:5f:d0:be"}}"#;
        let info = parse_device_info(body).expect("parses");
        assert_eq!(info.mac(), None, "ssid is not a MAC");
    }

    /// A document that parses but carries no `device` object is a firmware change, not
    /// an empty answer — reading it as "no MAC" would hide the change.
    #[test]
    fn a_document_without_a_device_object_is_a_protocol_error() {
        let error = parse_device_info(r#"{"version":"2.0.25"}"#).expect_err("no device object");
        assert!(matches!(error, crate::TizenError::Protocol(_)), "{error:?}");
    }

    #[test]
    fn a_non_json_body_is_a_json_error() {
        assert!(parse_device_info("<html>not json</html>").is_err());
    }

    /// The struct is what a caller stores, so an empty one must be inert rather than
    /// carrying a placeholder address.
    #[test]
    fn an_empty_device_info_has_no_address() {
        assert_eq!(DeviceInfo::default().mac(), None);
    }
}
