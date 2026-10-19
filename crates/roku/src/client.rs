//! Roku ECP HTTP client plus the network-free parsing and path helpers.
//!
//! All logic that can run without I/O (XML parsing, path construction, key
//! formatting) lives in free functions so it can be unit-tested offline.

use std::time::Duration;

use crate::error::{Result, RokuError};
use crate::payload::{RokuApp, RokuDeviceInfo};
use crate::types::{
    RokuKey, ECP_APPS_PATH, ECP_DEVICE_INFO_PATH, ECP_KEYPRESS_PATH_PREFIX, ECP_LAUNCH_PATH_PREFIX,
    ROKU_ECP_PORT,
};

/// Request timeout applied to every ECP command.
const ECP_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Returns the trimmed text of the first direct child element named `name`.
fn element_text(node: roxmltree::Node<'_, '_>, name: &str) -> Option<String> {
    node.children()
        .find(|child| child.is_element() && child.tag_name().name() == name)
        .and_then(|child| child.text())
        .map(|text| text.trim().to_string())
}

/// Parses a boolean child element; absent or unrecognised values become `None`.
fn element_bool(node: roxmltree::Node<'_, '_>, name: &str) -> Option<bool> {
    element_text(node, name).and_then(|value| match value.to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}

/// Parses a `<device-info>` XML document into [`RokuDeviceInfo`].
///
/// Hyphenated element names (`<vendor-name>`, `<screen-resolution>`, …) map to
/// the underscore fields of [`RokuDeviceInfo`]. Elements that are absent stay
/// `None`; unknown elements are ignored. Malformed XML yields [`RokuError::Xml`].
pub fn parse_device_info(xml: &str) -> Result<RokuDeviceInfo> {
    let document =
        roxmltree::Document::parse(xml).map_err(|err| RokuError::Xml(err.to_string()))?;
    let root = document.root_element();
    if root.tag_name().name() != "device-info" {
        return Err(RokuError::Xml(format!(
            "expected <device-info> root element, found <{}>",
            root.tag_name().name()
        )));
    }

    Ok(RokuDeviceInfo {
        vendor_name: element_text(root, "vendor-name"),
        model_name: element_text(root, "model-name"),
        model_number: element_text(root, "model-number"),
        serial_number: element_text(root, "serial-number"),
        device_id: element_text(root, "device-id"),
        wifi_mac: element_text(root, "wifi-mac"),
        ethernet_mac: element_text(root, "ethernet-mac"),
        power_mode: element_text(root, "power-mode"),
        screen_resolution: element_text(root, "screen-resolution"),
        software_version: element_text(root, "software-version"),
        user_device_name: element_text(root, "user-device-name"),
        supports_suspend: element_bool(root, "supports-suspend"),
        supports_find_remote: element_bool(root, "supports-find-remote"),
        supports_tv_power_control: element_bool(root, "supports-tv-power-control"),
        supports_audio_volume_control: element_bool(root, "supports-audio-volume-control"),
    })
}

/// Parses an `<apps>` XML document into a list of [`RokuApp`].
///
/// Each `<app>` contributes its `id` attribute and its trimmed element text as
/// the display name. Malformed XML yields [`RokuError::Xml`].
pub fn parse_apps(xml: &str) -> Result<Vec<RokuApp>> {
    let document =
        roxmltree::Document::parse(xml).map_err(|err| RokuError::Xml(err.to_string()))?;
    let apps = document
        .root_element()
        .children()
        .filter(|child| child.is_element() && child.tag_name().name() == "app")
        .map(|app| RokuApp {
            id: app.attribute("id").unwrap_or_default().to_string(),
            name: app
                .text()
                .map(|text| text.trim().to_string())
                .unwrap_or_default(),
        })
        .collect();
    Ok(apps)
}

/// Builds the ECP keypress path for `key`, e.g. `/keypress/PowerOff`.
pub fn keypress_path(key: RokuKey) -> String {
    let segment = key.as_str();
    format!("{ECP_KEYPRESS_PATH_PREFIX}{segment}")
}

/// Builds the ECP launch path for `app_id`, e.g. `/launch/12`.
pub fn launch_path(app_id: &str) -> String {
    format!("{ECP_LAUNCH_PATH_PREFIX}{app_id}")
}

/// HTTP client for the Roku ECP REST API on port 8060.
#[derive(Debug, Clone)]
pub struct RokuClient {
    base_url: String,
    http: reqwest::Client,
}

impl RokuClient {
    /// Creates a client targeting `host` on the default ECP port (8060).
    pub fn new(host: impl Into<String>) -> Self {
        let host = host.into();
        Self::with_base_url(format!("http://{host}:{ROKU_ECP_PORT}"))
    }

    /// Creates a client targeting an explicit base URL (no trailing slash, no
    /// port suffix added).
    pub fn with_base_url(url: impl Into<String>) -> Self {
        // `ClientBuilder::build` only fails when the TLS backend cannot be
        // initialised. ECP is plain HTTP, but the builder is still fallible, so
        // we degrade to the default client instead of panicking.
        let http = reqwest::Client::builder()
            .timeout(ECP_REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::default());
        Self {
            base_url: url.into(),
            http,
        }
    }

    /// Returns the base URL this client issues requests against.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Joins the base URL with an absolute ECP `path`.
    fn endpoint(&self, path: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        format!("{base}{path}")
    }

    /// Fetches and parses `GET /query/device-info`.
    pub async fn query_device_info(&self) -> Result<RokuDeviceInfo> {
        let url = self.endpoint(ECP_DEVICE_INFO_PATH);
        let body = self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        parse_device_info(&body)
    }

    /// Fetches and parses the installed application list from `GET /query/apps`.
    pub async fn query_apps(&self) -> Result<Vec<RokuApp>> {
        let url = self.endpoint(ECP_APPS_PATH);
        let body = self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        parse_apps(&body)
    }

    /// Sends a single remote-key press via `POST /keypress/{key}` with an empty body.
    ///
    /// # Errors
    ///
    /// Returns [`RokuError::NotPermitted`] when the device answers 403 — on Roku OS
    /// 14.1+ that means *Control by mobile apps* is disabled, and the error says so.
    pub async fn send_keypress(&self, key: RokuKey) -> Result<()> {
        self.post(&keypress_path(key)).await
    }

    /// `POST`s to `path`, translating the two answers that are not failures of this
    /// client into errors a caller can act on.
    async fn post(&self, path: &str) -> Result<()> {
        let response = self.http.post(self.endpoint(path)).send().await?;
        if response.status() == reqwest::StatusCode::FORBIDDEN {
            return Err(RokuError::NotPermitted {
                remedy: crate::types::CONTROL_BY_MOBILE_APPS_REMEDY.to_string(),
            });
        }
        response.error_for_status()?;
        Ok(())
    }

    /// Launches an application by id via `POST /launch/{app_id}` with an empty body.
    pub async fn launch_app(&self, app_id: &str) -> Result<()> {
        self.post(&launch_path(app_id)).await
    }

    /// Resolves an application id from its display name.
    ///
    /// Fetches the app list and returns the id of the first app whose name
    /// matches `name` case-insensitively. Returns [`RokuError::NotFound`] when
    /// no app matches.
    pub async fn query_app_id(&self, name: &str) -> Result<String> {
        let apps = self.query_apps().await?;
        apps.into_iter()
            .find(|app| app.name.eq_ignore_ascii_case(name))
            .map(|app| app.id)
            .ok_or_else(|| RokuError::NotFound(name.to_string()))
    }

    /// Powers the device off by sending the `PowerOff` keypress.
    ///
    /// Roku only supports `PowerOff` over ECP on most models.
    pub async fn power_off(&self) -> Result<()> {
        self.send_keypress(RokuKey::PowerOff).await
    }

    /// Attempts to power the device on by sending the `PowerOn` keypress.
    ///
    /// Most Roku models do **not** accept `PowerOn` over ECP while in deep
    /// standby; waking them requires Wake-on-LAN, which is what the
    /// `tv-rs-power` crate provides. This method is kept for models that do
    /// honour the keypress.
    pub async fn power_on(&self) -> Result<()> {
        self.send_keypress(RokuKey::PowerOn).await
    }

    /// Asks the device what its power state is.
    ///
    /// This is a query, not a probe: `GET /query/device-info` answers while the
    /// device is awake, and fails when it is genuinely off. A device whose
    /// firmware omits `<power-mode>` yields [`crate::RokuPowerMode::Unknown`], which is a
    /// successful answer to "what does it say?" and not an error.
    ///
    /// # Errors
    ///
    /// Returns [`crate::RokuError`] when the HTTP request fails or the body is not a
    /// `<device-info>` document.
    pub async fn power_state(&self) -> Result<crate::types::RokuPowerMode> {
        Ok(self.query_device_info().await?.power_state())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const DEVICE_INFO_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<device-info>
  <udn>00000000-0000-0000-0000-000000000000</udn>
  <serial-number>YN00C1234567</serial-number>
  <device-id>8f1e2d3c4b5a6978</device-id>
  <vendor-name>Roku</vendor-name>
  <model-name>Roku Ultra</model-name>
  <model-number>4800X</model-number>
  <wifi-mac>d8:31:34:aa:bb:cc</wifi-mac>
  <ethernet-mac>d8:31:34:aa:bb:cd</ethernet-mac>
  <power-mode>PowerOn</power-mode>
  <screen-resolution>1080p</screen-resolution>
  <software-version>12.5.0</software-version>
  <user-device-name>Living Room TV</user-device-name>
  <supports-suspend>true</supports-suspend>
  <supports-find-remote>false</supports-find-remote>
  <unknown-element>ignored</unknown-element>
</device-info>"#;

    const APPS_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<apps>
  <app id="12" type="appl" version="1.0">Netflix</app>
  <app id="13" type="appl" version="2.0">YouTube </app>
</apps>"#;

    #[test]
    fn parse_device_info_maps_all_fields() {
        let info = parse_device_info(DEVICE_INFO_XML).expect("fixture should parse");
        assert_eq!(info.vendor_name.as_deref(), Some("Roku"));
        assert_eq!(info.model_name.as_deref(), Some("Roku Ultra"));
        assert_eq!(info.model_number.as_deref(), Some("4800X"));
        assert_eq!(info.serial_number.as_deref(), Some("YN00C1234567"));
        assert_eq!(info.device_id.as_deref(), Some("8f1e2d3c4b5a6978"));
        assert_eq!(info.wifi_mac.as_deref(), Some("d8:31:34:aa:bb:cc"));
        assert_eq!(info.ethernet_mac.as_deref(), Some("d8:31:34:aa:bb:cd"));
        assert_eq!(info.power_mode.as_deref(), Some("PowerOn"));
        assert_eq!(info.screen_resolution.as_deref(), Some("1080p"));
        assert_eq!(info.software_version.as_deref(), Some("12.5.0"));
        assert_eq!(info.user_device_name.as_deref(), Some("Living Room TV"));
        assert_eq!(info.supports_suspend, Some(true));
        assert_eq!(info.supports_find_remote, Some(false));
    }

    #[test]
    fn parse_device_info_returns_xml_error_on_malformed_input() {
        let err = parse_device_info("<device-info><vendor-name>Roku</device-info>")
            .expect_err("mismatched tags must not parse");
        assert!(
            matches!(err, RokuError::Xml(_)),
            "expected Xml error, got {err:?}"
        );
    }

    #[test]
    fn parse_apps_reads_ids_and_trims_names() {
        let apps = parse_apps(APPS_XML).expect("apps fixture should parse");
        assert_eq!(apps.len(), 2);
        assert_eq!(
            apps[0],
            RokuApp {
                id: "12".to_string(),
                name: "Netflix".to_string(),
            }
        );
        assert_eq!(apps[1].id, "13");
        assert_eq!(apps[1].name, "YouTube");
    }

    #[test]
    fn keypress_and_launch_paths_are_exact() {
        assert_eq!(keypress_path(RokuKey::PowerOff), "/keypress/PowerOff");
        assert_eq!(launch_path("12"), "/launch/12");
    }

    #[test]
    fn roku_key_as_str_returns_exact_ecp_names() {
        assert_eq!(RokuKey::Home.as_str(), "Home");
        assert_eq!(RokuKey::InstantReplay.as_str(), "InstantReplay");
        assert_eq!(RokuKey::VolumeUp.as_str(), "VolumeUp");
        assert_eq!(RokuKey::Back.as_str(), "Back");
    }

    #[tokio::test]
    async fn query_device_info_succeeds_against_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(ECP_DEVICE_INFO_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_raw(DEVICE_INFO_XML, "text/xml"))
            .mount(&server)
            .await;

        let client = RokuClient::with_base_url(server.uri());
        let info = client
            .query_device_info()
            .await
            .expect("device-info request should succeed");
        assert_eq!(info.model_name.as_deref(), Some("Roku Ultra"));
    }

    #[tokio::test]
    async fn send_keypress_succeeds_against_mock() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/keypress/PowerOff"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = RokuClient::with_base_url(server.uri());
        client
            .send_keypress(RokuKey::PowerOff)
            .await
            .expect("keypress request should succeed");
    }

    #[tokio::test]
    async fn launch_app_succeeds_against_mock() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/launch/12"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;

        let client = RokuClient::with_base_url(server.uri());
        client
            .launch_app("12")
            .await
            .expect("launch request should succeed");
    }

    #[tokio::test]
    async fn query_app_id_resolves_case_insensitively() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path(ECP_APPS_PATH))
            .respond_with(ResponseTemplate::new(200).set_body_raw(APPS_XML, "text/xml"))
            .mount(&server)
            .await;

        let client = RokuClient::with_base_url(server.uri());
        let id = client
            .query_app_id("netflix")
            .await
            .expect("app lookup should succeed");
        assert_eq!(id, "12");
    }

    #[test]
    fn os_15_capability_flags_are_parsed() {
        let xml = r#"<device-info>
            <vendor-name>Roku</vendor-name>
            <supports-tv-power-control>true</supports-tv-power-control>
            <supports-audio-volume-control>false</supports-audio-volume-control>
        </device-info>"#;
        let info = parse_device_info(xml).expect("parses");
        assert_eq!(info.supports_tv_power_control, Some(true));
        assert_eq!(info.supports_audio_volume_control, Some(false));

        let without =
            parse_device_info("<device-info><vendor-name>Roku</vendor-name></device-info>")
                .expect("parses");
        assert_eq!(
            without.supports_tv_power_control, None,
            "an old firmware simply omits the flag"
        );
    }

    #[tokio::test]
    async fn a_forbidden_keypress_names_the_setting_to_enable() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/keypress/Home"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let client = RokuClient::with_base_url(server.uri());
        let error = client.send_keypress(RokuKey::Home).await.expect_err("403");

        match error {
            RokuError::NotPermitted { remedy } => {
                assert!(remedy.contains("Control by mobile apps"), "{remedy}");
            }
            other => panic!("expected NotPermitted, got {other:?}"),
        }
    }
}
