//! `DeviceHandle` for the facade's [`TvClient`].
//!
//! The enum stays the natural type for "one TV, vendor known". This module gives
//! it the second identity a fleet needs: the erased traits from `tv-rs-core`, so a
//! caller can hold `Arc<dyn DeviceHandle>` without knowing which backend is behind
//! it.
//!
//! # Capability honesty
//!
//! [`TvClient`] advertises only what it can actually do today, per vendor:
//!
//! | Vendor | power | wake | key | app | foreground | input | volume | panel lock | orientation | panel health |
//! |---|---|---|---|---|---|---|---|---|---|---|
//! | Tizen | ✓ | — | ✓ | ✓ | — | — | — | — | — | — |
//! | webOS | ✓ | — | — | ✓ | ✓ | — | ✓ | — | — | — |
//! | Roku | ✓ | — | ✓ | ✓ | — | — | — | — | — | — |
//!
//! `wake` is absent because a discovered device does not carry its MAC yet, and a
//! wake without one is a lie. `key` is absent on webOS because SSAP has no remote
//! key endpoint — it wants a pointer-input socket. `volume` is absent on Tizen and
//! Roku because both expose relative keys, not an absolute level, so `set_volume`
//! cannot be honoured. The LFD columns are absent everywhere **per protocol**: the
//! consumer remote channels (Tizen 8001/8002, SSAP, ECP) have no panel-lock,
//! orientation or health command — a commercial panel is driven over MDC
//! (`as_panel_lock()` / `as_orientation()` / `as_panel_health()` are honest on
//! MDC-backed handles, `None` here), and the facade opts out by omission, the
//! same way the MDC adapter opts out of screen capture.

use async_trait::async_trait;

use tv_rs_core::{
    AppLaunch, AppRef, Capability, CapabilitySet, DeviceDescriptor, DeviceHandle, DeviceId,
    DeviceOs, Input, InputSelect, Key, KeyInput, PowerControl, PowerState, TvError, VolumeControl,
};
use tv_rs_roku::RokuKey;
use tv_rs_tizen::{TizenRemoteKey, TizenSession};

use crate::client::TvClient;
use crate::map;
use crate::types::TvVendor;

impl TvClient {
    /// The capabilities this backend actually implements.
    #[must_use]
    pub fn core_capabilities(&self) -> CapabilitySet {
        let mut set = CapabilitySet::empty().with(Capability::PowerState);
        match self {
            Self::Tizen(_) => {
                set.insert(Capability::Key);
                set.insert(Capability::AppLaunch);
            }
            Self::Webos(_) => {
                set.insert(Capability::AppLaunch);
                set.insert(Capability::Foreground);
                set.insert(Capability::Volume);
            }
            Self::Roku(_) => {
                set.insert(Capability::Key);
                set.insert(Capability::AppLaunch);
            }
        }
        set
    }
}

impl DeviceHandle for TvClient {
    fn id(&self) -> DeviceId {
        // Discovery normally supplies a UDN; a client built from an address alone
        // gets the host, which is still a stable key for a static-IP install.
        DeviceId::new(host_of(self.address())).unwrap_or_else(|_| DeviceId::anonymous())
    }

    fn descriptor(&self) -> DeviceDescriptor {
        DeviceDescriptor::new(self.id(), device_os(self.vendor()), host_of(self.address()))
    }

    fn capabilities(&self) -> CapabilitySet {
        self.core_capabilities()
    }

    fn as_power(&self) -> Option<&dyn PowerControl> {
        Some(self)
    }

    fn as_key(&self) -> Option<&dyn KeyInput> {
        self.core_capabilities()
            .contains(Capability::Key)
            .then_some(self as &dyn KeyInput)
    }

    fn as_app(&self) -> Option<&dyn AppLaunch> {
        Some(self)
    }

    fn as_input(&self) -> Option<&dyn InputSelect> {
        None
    }

    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        self.core_capabilities()
            .contains(Capability::Volume)
            .then_some(self as &dyn VolumeControl)
    }
}

#[async_trait]
impl PowerControl for TvClient {
    async fn power_state(&self) -> Result<PowerState, TvError> {
        // Fully qualified: the inherent method is the implementation, and calling
        // `self.power_state()` here would recurse into this trait method.
        // The inherent method already answers in the core taxonomy, so the error
        // crosses as-is.
        let state = TvClient::power_state(self).await?;
        Ok(core_power_state(state))
    }

    async fn wake(&self) -> Result<(), TvError> {
        Err(TvError::Unsupported(Capability::Wake))
    }

    async fn sleep(&self) -> Result<(), TvError> {
        TvClient::power_off(self).await
    }
}

#[async_trait]
impl KeyInput for TvClient {
    async fn key_press(&self, key: Key, _cmd: tv_rs_core::CommandId) -> Result<(), TvError> {
        match self {
            Self::Tizen(client) => {
                let key = tizen_key(key)?;
                let mut session = TizenSession::connect(client)
                    .await
                    .map_err(|error| map::tizen(&error, Capability::Key))?;
                session
                    .send_key(key)
                    .await
                    .map_err(|error| map::tizen(&error, Capability::Key))
            }
            Self::Roku(client) => {
                let key = roku_key(key)?;
                client
                    .send_keypress(key)
                    .await
                    .map_err(|error| map::roku(&error, Capability::Key))
            }
            Self::Webos(_) => Err(TvError::Unsupported(Capability::Key)),
        }
    }
}

#[async_trait]
impl AppLaunch for TvClient {
    async fn launch(&self, app: AppRef, _cmd: tv_rs_core::CommandId) -> Result<(), TvError> {
        TvClient::launch_app(self, &app.id).await
    }

    async fn foreground(&self) -> Result<Option<AppRef>, TvError> {
        match self {
            Self::Webos(client) => {
                let mut session = client
                    .connect()
                    .await
                    .map_err(|error| map::webos(&error, Capability::Foreground))?;
                let app_id = session
                    .foreground_app()
                    .await
                    .map_err(|error| map::webos(&error, Capability::Foreground))?;
                Ok(app_id.map(AppRef::new))
            }
            _ => Err(TvError::Unsupported(Capability::Foreground)),
        }
    }
}

#[async_trait]
impl InputSelect for TvClient {
    async fn active_input(&self) -> Result<Option<Input>, TvError> {
        Err(TvError::Unsupported(Capability::InputSelect))
    }

    async fn select_input(
        &self,
        _input: Input,
        _cmd: tv_rs_core::CommandId,
    ) -> Result<(), TvError> {
        Err(TvError::Unsupported(Capability::InputSelect))
    }
}

#[async_trait]
impl VolumeControl for TvClient {
    async fn volume(&self) -> Result<Option<u8>, TvError> {
        match self {
            Self::Webos(client) => {
                let mut session = client
                    .connect()
                    .await
                    .map_err(|error| map::webos(&error, Capability::Volume))?;
                session
                    .get_volume()
                    .await
                    .map(Some)
                    .map_err(|error| map::webos(&error, Capability::Volume))
            }
            _ => Err(TvError::Unsupported(Capability::Volume)),
        }
    }

    async fn set_volume(&self, level: u8, _cmd: tv_rs_core::CommandId) -> Result<(), TvError> {
        match self {
            Self::Webos(client) => {
                let mut session = client
                    .connect()
                    .await
                    .map_err(|error| map::webos(&error, Capability::Volume))?;
                session
                    .set_volume(level)
                    .await
                    .map_err(|error| map::webos(&error, Capability::Volume))
            }
            _ => Err(TvError::Unsupported(Capability::Volume)),
        }
    }

    async fn set_mute(&self, muted: bool, _cmd: tv_rs_core::CommandId) -> Result<(), TvError> {
        match self {
            Self::Webos(client) => {
                let mut session = client
                    .connect()
                    .await
                    .map_err(|error| map::webos(&error, Capability::Volume))?;
                session
                    .set_mute(muted)
                    .await
                    .map_err(|error| map::webos(&error, Capability::Volume))
            }
            _ => Err(TvError::Unsupported(Capability::Volume)),
        }
    }
}

/// Extracts the bare host from whatever an address-like field holds.
///
/// The three backends store different shapes (`http://10.0.0.4:8060`, `10.0.0.4`),
/// and an identity that changes with the scheme is not an identity.
fn host_of(address: &str) -> &str {
    let without_scheme = address
        .strip_prefix("http://")
        .or_else(|| address.strip_prefix("https://"))
        .or_else(|| address.strip_prefix("ws://"))
        .or_else(|| address.strip_prefix("wss://"))
        .unwrap_or(address);
    match without_scheme.rsplit_once(':') {
        Some((host, port)) if port.parse::<u16>().is_ok() => host,
        _ => without_scheme,
    }
}

/// Converts the power crate's vocabulary into the core's.
///
/// The two enums are identical today; the conversion exists so the core owns the
/// type and a backend can never widen it by accident.
fn core_power_state(state: tv_rs_power::PowerState) -> PowerState {
    match state {
        tv_rs_power::PowerState::On => PowerState::On,
        tv_rs_power::PowerState::Standby => PowerState::Standby,
        tv_rs_power::PowerState::Off => PowerState::Off,
        tv_rs_power::PowerState::Unknown => PowerState::Unknown,
    }
}

fn device_os(vendor: TvVendor) -> DeviceOs {
    match vendor {
        TvVendor::Samsung => DeviceOs::Tizen,
        TvVendor::Lg => DeviceOs::WebOs,
        TvVendor::Roku => DeviceOs::Roku,
        TvVendor::Unknown => DeviceOs::Unknown,
    }
}

/// Maps a core key onto the Tizen remote-key vocabulary.
///
/// Keys with no Tizen equivalent are reported as `Unsupported` rather than being
/// approximated by a different key: pressing "menu" when the caller asked for
/// "info" is worse than a refusal.
fn tizen_key(key: Key) -> Result<TizenRemoteKey, TvError> {
    Ok(match key {
        Key::Up => TizenRemoteKey::Up,
        Key::Down => TizenRemoteKey::Down,
        Key::Left => TizenRemoteKey::Left,
        Key::Right => TizenRemoteKey::Right,
        Key::Select | Key::Enter => TizenRemoteKey::Enter,
        Key::Back => TizenRemoteKey::Return,
        Key::Home => TizenRemoteKey::Home,
        Key::Menu => TizenRemoteKey::Menu,
        Key::Play => TizenRemoteKey::Play,
        Key::Pause => TizenRemoteKey::Pause,
        Key::Stop => TizenRemoteKey::Stop,
        Key::Rewind => TizenRemoteKey::Rewind,
        Key::FastForward => TizenRemoteKey::FastForward,
        Key::ChannelUp => TizenRemoteKey::ChannelUp,
        Key::ChannelDown => TizenRemoteKey::ChannelDown,
        Key::VolumeUp => TizenRemoteKey::VolumeUp,
        Key::VolumeDown => TizenRemoteKey::VolumeDown,
        Key::Mute => TizenRemoteKey::Mute,
        Key::Power => TizenRemoteKey::Power,
        Key::PowerOff => TizenRemoteKey::PowerOff,
        _ => return Err(TvError::Unsupported(Capability::Key)),
    })
}

/// Maps a core key onto the Roku ECP key vocabulary.
fn roku_key(key: Key) -> Result<RokuKey, TvError> {
    Ok(match key {
        Key::Up => RokuKey::Up,
        Key::Down => RokuKey::Down,
        Key::Left => RokuKey::Left,
        Key::Right => RokuKey::Right,
        Key::Select | Key::Enter => RokuKey::Select,
        Key::Back => RokuKey::Back,
        Key::Home => RokuKey::Home,
        Key::Play => RokuKey::Play,
        Key::Rewind => RokuKey::Rev,
        Key::FastForward => RokuKey::Fwd,
        Key::Info => RokuKey::Info,
        Key::Search => RokuKey::Search,
        Key::Backspace => RokuKey::Backspace,
        Key::ChannelUp => RokuKey::ChannelUp,
        Key::ChannelDown => RokuKey::ChannelDown,
        Key::VolumeUp => RokuKey::VolumeUp,
        Key::VolumeDown => RokuKey::VolumeDown,
        Key::Mute => RokuKey::VolumeMute,
        Key::Power => RokuKey::PowerOn,
        Key::PowerOff => RokuKey::PowerOff,
        _ => return Err(TvError::Unsupported(Capability::Key)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tv_rs_roku::RokuClient;
    use tv_rs_tizen::TizenClient;
    use tv_rs_webos::WebosClient;

    fn tizen() -> TvClient {
        TvClient::Tizen(TizenClient::new("10.0.0.2"))
    }

    fn webos() -> TvClient {
        TvClient::Webos(WebosClient::new("10.0.0.3"))
    }

    fn roku() -> TvClient {
        TvClient::Roku(RokuClient::new("10.0.0.4"))
    }

    #[test]
    fn capability_sets_match_the_implementations() {
        for (client, expected) in [
            (
                tizen(),
                vec![
                    Capability::PowerState,
                    Capability::Key,
                    Capability::AppLaunch,
                ],
            ),
            (
                webos(),
                vec![
                    Capability::PowerState,
                    Capability::AppLaunch,
                    Capability::Foreground,
                    Capability::Volume,
                ],
            ),
            (
                roku(),
                vec![
                    Capability::PowerState,
                    Capability::Key,
                    Capability::AppLaunch,
                ],
            ),
        ] {
            assert_eq!(client.capabilities().iter().collect::<Vec<_>>(), expected);
        }
    }

    #[test]
    fn accessors_agree_with_the_advertised_set() {
        for client in [tizen(), webos(), roku()] {
            let set = client.capabilities();
            assert!(
                !set.contains(Capability::Wake),
                "no backend can wake without a known MAC"
            );
            assert_eq!(set.contains(Capability::Key), client.as_key().is_some());
            assert!(client.as_app().is_some());
            assert_eq!(
                set.contains(Capability::Volume),
                client.as_volume().is_some()
            );
            assert!(
                client.as_input().is_none(),
                "no backend implements input yet"
            );
        }
    }

    #[test]
    fn identity_falls_back_to_the_address() {
        let client = roku();
        assert_eq!(client.id().as_str(), "10.0.0.4");
        assert_eq!(client.descriptor().os, DeviceOs::Roku);
        assert_eq!(client.descriptor().address, "10.0.0.4");
    }

    #[tokio::test]
    async fn wake_is_refused_rather_than_faked() {
        let client = tizen();
        let error = PowerControl::wake(&client).await.expect_err("no MAC known");
        assert!(matches!(error, TvError::Unsupported(Capability::Wake)));
    }

    #[tokio::test]
    async fn webos_refuses_keys_without_touching_the_network() {
        let client = webos();
        let error = KeyInput::key_press(&client, Key::Home, tv_rs_core::CommandId::new())
            .await
            .expect_err("webOS has no key endpoint");
        assert!(matches!(error, TvError::Unsupported(Capability::Key)));
    }

    #[tokio::test]
    async fn input_selection_is_unsupported_everywhere() {
        let client = roku();
        assert!(matches!(
            InputSelect::select_input(&client, Input::Hdmi(1), tv_rs_core::CommandId::new())
                .await
                .expect_err("not implemented"),
            TvError::Unsupported(Capability::InputSelect)
        ));
    }

    #[test]
    fn key_mappings_cover_the_common_set_and_refuse_the_rest() {
        assert_eq!(tizen_key(Key::Select).unwrap(), TizenRemoteKey::Enter);
        assert_eq!(roku_key(Key::Mute).unwrap(), RokuKey::VolumeMute);
        assert!(tizen_key(Key::Digit(1)).is_err());
        assert!(roku_key(Key::Pause).is_err());
    }

    #[test]
    fn errors_route_to_actions_not_vendors() {
        // The refusal to do an operation the vendor has no protocol message for:
        let restricted = TvError::UnsupportedOperation {
            vendor: "roku",
            operation: "input_select",
        };
        assert!(matches!(
            restricted,
            TvError::UnsupportedOperation { vendor: "roku", .. }
        ));

        // A Tizen that rejected the pairing prompt routes to pairing-required,
        // not to the old catch-all's `Wait`.
        let pairing = map::tizen(&tv_rs_tizen::TizenError::Unauthorized, Capability::Key);
        assert!(matches!(
            pairing,
            TvError::Auth(tv_rs_core::AuthError::PairingRequired)
        ));
    }

    #[test]
    fn a_caller_supplied_policy_reaches_the_backend() {
        use crate::types::DiscoveredTv;
        use tv_rs_transport::{CertFingerprint, TlsPolicy};

        let fingerprint = CertFingerprint::from_hex(&"ab".repeat(32)).expect("valid fingerprint");
        let discovered = DiscoveredTv {
            vendor: TvVendor::Lg,
            ip: "10.0.0.9".to_string(),
            location: None,
            friendly_name: None,
            manufacturer: None,
            model_name: None,
            serial_number: None,
            usn: None,
        };

        let client = TvClient::from_discovered_with(&discovered, TlsPolicy::Pinned(fingerprint))
            .expect("lg");
        assert_eq!(
            client.as_webos().expect("webos").tls_policy(),
            TlsPolicy::Pinned(fingerprint)
        );

        let default = TvClient::from_discovered(&discovered).expect("lg");
        assert_eq!(
            default.as_webos().expect("webos").tls_policy(),
            TlsPolicy::Strict,
            "the default stays strict"
        );

        // Roku is plain HTTP on the LAN: the policy is accepted and ignored.
        let roku = DiscoveredTv {
            vendor: TvVendor::Roku,
            ..discovered
        };
        assert!(TvClient::from_discovered_with(&roku, TlsPolicy::Pinned(fingerprint)).is_ok());
    }

    #[tokio::test]
    async fn open_url_is_refused_where_there_is_no_browser() {
        let tizen = tizen();
        assert!(matches!(
            tizen
                .open_url("http://10.0.0.9:8080/player")
                .await
                .expect_err("no url launcher"),
            TvError::UnsupportedOperation {
                vendor: "samsung",
                ..
            }
        ));

        let roku = roku();
        assert!(matches!(
            roku.open_url("http://10.0.0.9:8080/player")
                .await
                .expect_err("no browser"),
            TvError::UnsupportedOperation { vendor: "roku", .. }
        ));
    }

    #[cfg(feature = "conformance")]
    #[tokio::test]
    async fn the_facade_passes_the_conformance_suite() {
        for client in [tizen(), webos(), roku()] {
            let violations = tv_rs_core::conformance::run(&client).await;
            assert!(violations.is_empty(), "{:?}", violations);
        }
    }

    #[test]
    fn a_roku_permission_failure_routes_to_reprovisioning() {
        let error = map::roku(
            &tv_rs_roku::RokuError::NotPermitted {
                remedy: "enable Control by mobile apps".to_string(),
            },
            Capability::Key,
        );
        match error {
            TvError::Auth(tv_rs_core::AuthError::NotPermitted { remedy }) => {
                assert!(remedy.contains("Control by mobile apps"), "{remedy}");
            }
            other => panic!("expected Auth(NotPermitted), got {other:?}"),
        }
    }
}
