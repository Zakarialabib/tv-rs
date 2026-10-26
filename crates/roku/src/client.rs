//! Roku ECP HTTP client plus the network-free parsing and path helpers.
//!
//! All logic that can run without I/O (XML parsing, path construction, key
//! formatting) lives in free functions so it can be unit-tested offline.

use std::time::Duration;

use tv_rs_transport::Unsupported;

use crate::error::{Result, RokuError};
use crate::payload::{RokuActiveApp, RokuApp, RokuDeviceInfo};
use crate::types::{
    RokuKey, ECP_ACTIVE_APP_PATH, ECP_APPS_PATH, ECP_DEVICE_INFO_PATH, ECP_KEYPRESS_PATH_PREFIX,
    ECP_LAUNCH_PATH_PREFIX, ROKU_ECP_PORT,
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

/// Parses an `<active-app>` XML document into [`RokuActiveApp`].
///
/// Malformed XML, or a document whose root is not `<active-app>`, yields
/// [`RokuError::Xml`]. A missing `app` element is treated as "nothing in front"
/// rather than an error: the shape is the device's, and refusing to answer would be
/// worse than an empty answer.
pub fn parse_active_app(xml: &str) -> Result<RokuActiveApp> {
    let document =
        roxmltree::Document::parse(xml).map_err(|err| RokuError::Xml(err.to_string()))?;
    let root = document.root_element();
    if root.tag_name().name() != "active-app" {
        return Err(RokuError::Xml(format!(
            "expected <active-app> root element, found <{}>",
            root.tag_name().name()
        )));
    }

    let app = root
        .children()
        .find(|child| child.is_element() && child.tag_name().name() == "app");

    Ok(RokuActiveApp {
        app_id: app
            .and_then(|node| node.attribute("id"))
            .map(str::to_string),
        app_name: app
            .and_then(|node| node.text())
            .map(|text| text.trim().to_string())
            .unwrap_or_default(),
        screensaver: element_text(root, "screensaver"),
    })
}

/// Builds the `Lit_<char>` path for a character the documented surface can carry.
///
/// Roku's ECP documentation covers printable ASCII ("Printable ASCII character code
/// values can be transmitted as-is with the `Lit_` prefix"); anything else has no
/// documented encoding here, so this answers `None` and the caller refuses rather
/// than guess at a percent-encoding a search box would have to explain.
#[must_use]
fn literal_path(ch: char) -> Option<String> {
    (ch.is_ascii_graphic() || ch == ' ').then(|| format!("{ECP_KEYPRESS_PATH_PREFIX}Lit_{ch}"))
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

    /// Fetches and parses the application currently in front from
    /// `GET /query/active-app`.
    ///
    /// # Errors
    ///
    /// Returns [`RokuError::Http`] when the device does not answer and
    /// [`RokuError::Xml`] when the reply is not an `<active-app>` document.
    pub async fn query_active_app(&self) -> Result<RokuActiveApp> {
        let url = self.endpoint(ECP_ACTIVE_APP_PATH);
        let body = self
            .http
            .get(url)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        parse_active_app(&body)
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

    /// Types `text` into the focused field, one `Lit_<char>` request per character.
    ///
    /// Roku's ECP documentation: "The key parameter can either be a key name … or a
    /// printable character value specified with the prefix `Lit_`. Printable ASCII
    /// character code values can be transmitted as-is with the `Lit_` prefix." That
    /// documented surface is printable ASCII; the encoding for anything else is not
    /// documented in that source, so this refuses it rather than guessing at a
    /// percent-encoding that a search box would then have to explain.
    ///
    /// # Errors
    ///
    /// Returns [`RokuError::Unsupported`] when `text` holds a character the
    /// documented surface cannot carry, and [`RokuError::Http`] when the device
    /// refuses a request.
    pub async fn type_text(&self, text: &str) -> Result<()> {
        // Validate the whole string before sending anything: refusing halfway
        // through would leave half a search term typed into the field, which is
        // worse than not having typed at all.
        let paths: Vec<String> = text
            .chars()
            .map(|ch| {
                literal_path(ch).ok_or_else(|| {
                    RokuError::Unsupported(Unsupported::new(
                        3,
                        "non-ASCII characters are outside the documented printable-ASCII `Lit_` surface",
                    ))
                })
            })
            .collect::<Result<Vec<String>>>()?;

        for path in &paths {
            self.post(path).await?;
        }
        Ok(())
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
    /// The documented shape when an application is in front.
    #[test]
    fn an_active_app_carries_its_id() {
        let xml =
            r#"<active-app><app id="12" type="appl" version="4.1.218">Netflix</app></active-app>"#;
        let active = parse_active_app(xml).expect("valid");
        assert_eq!(active.app_id.as_deref(), Some("12"));
        assert_eq!(active.app_name, "Netflix");
        assert_eq!(active.screensaver, None);
    }

    /// The home screen is the bare text `Roku` with no id — nothing is in front,
    /// which is why the id is the optional half of the answer.
    #[test]
    fn the_home_screen_is_not_an_application() {
        let active = parse_active_app("<active-app><app>Roku</app></active-app>").expect("valid");
        assert_eq!(active.app_id, None);
        assert_eq!(active.app_name, "Roku");
    }

    /// A screensaver arrives beside the app it covers, not instead of it.
    #[test]
    fn a_screensaver_is_reported_next_to_the_app() {
        let xml =
            "<active-app><app>Roku</app><screensaver>Default screensaver</screensaver></active-app>";
        let active = parse_active_app(xml).expect("valid");
        assert_eq!(active.app_id, None);
        assert_eq!(active.screensaver.as_deref(), Some("Default screensaver"));
    }

    /// A different document is a protocol error, not an empty answer.
    #[test]
    fn a_non_active_app_document_is_rejected() {
        assert!(parse_active_app("<apps></apps>").is_err());
        assert!(parse_active_app("not xml at all").is_err());
    }
    /// Each character becomes its own `Lit_` request, in order.
    #[test]
    fn a_literal_path_is_the_character() {
        assert_eq!(literal_path('r').as_deref(), Some("/keypress/Lit_r"));
        assert_eq!(literal_path(' ').as_deref(), Some("/keypress/Lit_ "));
        assert_eq!(literal_path('7').as_deref(), Some("/keypress/Lit_7"));
    }

    /// The documented surface is printable ASCII; everything else has no encoding
    /// here and is refused rather than guessed at.
    #[test]
    fn a_character_outside_the_documented_surface_is_refused() {
        assert!(
            literal_path('é').is_none(),
            "non-ASCII has no documented encoding"
        );
        assert!(
            literal_path('\n').is_none(),
            "a control character is not printable"
        );
        assert!(
            literal_path('\u{20ac}').is_none(),
            "the euro sign is not ASCII"
        );
    }

    /// `type_text` refuses the whole string, so a search box never silently drops
    /// the character it could not send.
    #[tokio::test]
    async fn typing_refuses_a_string_holding_one_bad_character() {
        let client = RokuClient::new("10.0.0.4");
        let error = client
            .type_text("caf\u{e9}")
            .await
            .expect_err("é is refused");
        assert!(matches!(error, RokuError::Unsupported(_)), "{error:?}");
    }

    #[test]
    fn parse_apps_handles_malformed_xml_and_unexpected_nodes() {
        assert!(parse_apps("not xml").is_err());
        assert!(parse_apps("<apps><app>No ID</app></apps>").map(|apps| apps[0].id.is_empty()).unwrap_or(false));
        let xml = r#"<apps>
            <!-- comment -->
            <other-node>test</other-node>
            <app id="100">App Name</app>
        </apps>"#;
        let apps = parse_apps(xml).expect("parsed with unexpected child nodes");
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].id, "100");
        assert_eq!(apps[0].name, "App Name");
    }

    #[test]
    fn parse_active_app_handles_malformed_xml_and_unexpected_nodes() {
        assert!(parse_active_app("not xml").is_err());
        let err = parse_active_app("<wrong-root></wrong-root>").expect_err("wrong root tag");
        assert!(matches!(err, RokuError::Xml(_)));

        // Missing app tag inside <active-app> returns default empty active app, not error
        let active = parse_active_app("<active-app></active-app>").expect("missing app element");
        assert_eq!(active.app_id, None);
        assert_eq!(active.app_name, "");

        // Extra unexpected tags inside active-app
        let xml = r#"<active-app>
            <extra-node>val</extra-node>
            <app id="50">App 50</app>
        </active-app>"#;
        let active = parse_active_app(xml).expect("valid parse");
        assert_eq!(active.app_id.as_deref(), Some("50"));
        assert_eq!(active.app_name, "App 50");
    }
}
