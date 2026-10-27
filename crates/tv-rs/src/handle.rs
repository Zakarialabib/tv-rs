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
//! | Vendor | power | wake | key | text | app | app list | foreground | input | volume | panel lock | orientation | panel health |
//! |---|---|---|---|---|---|---|---|---|---|---|---|---|---|
//! | Tizen | ✓ | MAC | ✓ | ✓ | ✓ | — | — | — | — | — | — | — |
//! | webOS | ✓ | MAC | ✓ | ✓ | ✓ | ✓ | ✓ | — | ✓ | — | — | — |
//! | Roku | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | ✓ | — | — | — | — | — |
//!
//! `wake` reads `MAC` where the vendor drops its network stack when it sleeps:
//! the only wake path there is a broadcast magic packet, which needs an address
//! neither SSDP nor the pairing token carries — so the capability appears once
//! [`TvClient::with_mac`] has supplied one. Roku answers power-on over ECP while
//! the screen is dark, so it needs neither. `text` is ✓ on all three, each over a
//! different surface: Roku presses `Lit_<char>` per character, webOS calls its IME
//! with plain UTF-8, Tizen base64s the string into `SendInputString`. Three frames,
//! one operation — which is why the capability is advertised uniformly and only the
//! payload differs. `volume` is absent on Tizen and
//! Roku because both expose relative keys, not an absolute level, so `set_volume`
//! cannot be honoured. The LFD columns are absent everywhere **per protocol**: the
//! consumer remote channels (Tizen 8001/8002, SSAP, ECP) have no panel-lock,
//! orientation or health command — a commercial panel is driven over MDC
//! (`as_panel_lock()` / `as_orientation()` / `as_panel_health()` are honest on
//! MDC-backed handles, `None` here), and the facade opts out by omission, the
//! same way the MDC adapter opts out of screen capture.

use async_trait::async_trait;

use tv_rs_core::ProtocolError;
use tv_rs_core::{
    AppLaunch, AppRef, Capability, CapabilitySet, CommandId, DeviceDescriptor, DeviceHandle,
    DeviceId, DeviceOs, Input, InputSelect, Key, KeyInput, PowerControl, PowerState,
    ScreenCastProvider, TextInput, TvError, VolumeControl,
};
use tv_rs_power::MacAddress;
use tv_rs_roku::RokuKey;
use tv_rs_tizen::TizenRemoteKey;

use crate::client::TvClient;
use crate::dedupe;
use crate::map;
use crate::types::TvVendor;

impl TvClient {
    /// The capabilities this backend actually implements.
    #[must_use]
    pub fn core_capabilities(&self) -> CapabilitySet {
        let mut set = CapabilitySet::empty().with(Capability::PowerState);
        match self {
            Self::Tizen { .. } => {
                set.insert(Capability::Key);
                set.insert(Capability::AppLaunch);
                // `ms.remote.control` with `SendInputString` types a string.
                set.insert(Capability::TextInput);
            }
            Self::Webos { .. } => {
                set.insert(Capability::AppLaunch);
                set.insert(Capability::Foreground);
                set.insert(Capability::Volume);
                set.insert(Capability::Key);
                set.insert(Capability::InputSelect);
                // `ssap://com.webos.applicationManager/listApps` answers with the
                // installed catalogue, so this is a fact rather than a promise.
                set.insert(Capability::AppList);
                // `ssap://com.webos.service.ime/insertText` types into the focused field.
                set.insert(Capability::TextInput);
            }
            Self::Roku { .. } => {
                set.insert(Capability::Key);
                set.insert(Capability::AppLaunch);
                // ECP answers power-on while the screen is dark, so a Roku needs
                // neither a MAC nor a magic packet to be woken.
                set.insert(Capability::Wake);
                // `/query/active-app` says what is in front, which is the whole
                // input of an "on screen" card.
                set.insert(Capability::Foreground);
                // `/query/apps` lists what is installed.
                set.insert(Capability::AppList);
                // `keypress/Lit_<char>` types printable ASCII.
                set.insert(Capability::TextInput);
            }
        }
        // The other two drop their network stack when they sleep, so a magic
        // packet is the only path — a capability only once there is an address to
        // send it to.
        if self.mac().is_some() {
            set.insert(Capability::Wake);
        }
        set
    }

    /// The hardware address this handle knows, if any.
    #[must_use]
    pub fn mac(&self) -> Option<&str> {
        match self {
            Self::Tizen { mac, .. } | Self::Webos { mac, .. } | Self::Roku { mac, .. } => {
                mac.as_deref()
            }
        }
    }

    /// Remember the panel's hardware address.
    ///
    /// A magic packet is addressed to a MAC and neither the SSDP reply nor the
    /// pairing token carries one, so a fleet learns it once — from the TV's own
    /// network settings, or from [`TvClient::learn_mac`] where the vendor answers
    /// — and hands it back here. With it, `wake()` broadcasts a packet for the
    /// vendors that need one, and `descriptor().mac` reports it.
    #[must_use]
    pub fn with_mac(mut self, mac: impl Into<String>) -> Self {
        let mac = mac.into();
        match &mut self {
            Self::Tizen { mac: slot, .. }
            | Self::Webos { mac: slot, .. }
            | Self::Roku { mac: slot, .. } => *slot = Some(mac),
        }
        self
    }

    /// Ask the TV for its own hardware address, and remember it.
    ///
    /// Roku answers from `/query/device-info`, preferring the Wi-Fi address over the
    /// wired one; Tizen answers from `GET /api/v2/`, which needs no pairing at all.
    ///
    /// On Samsung the reported address is the Wi-Fi one **even when the set is wired**,
    /// so a wired set may not answer a magic packet sent to it — `device_info()` exposes
    /// `network_type` for a caller that needs to tell them apart.
    ///
    /// webOS refuses, and that is a vendor answer rather than a gap: its
    /// `getSystemInfo` key set is closed and carries no MAC, LG staff have said twice
    /// that no MAC API is provided, and the one endpoint that does carry one is
    /// open-source-only and gated at API level 29. Recorded in `docs/02-playbook.md`
    /// §4.
    ///
    /// # Errors
    ///
    /// Returns the mapped vendor error when the query fails, and
    /// [`TvError::UnsupportedOperation`] where no verified query exists.
    pub async fn learn_mac(&mut self) -> Result<Option<String>, TvError> {
        match self {
            Self::Roku { client, mac } => {
                let info = client
                    .query_device_info()
                    .await
                    .map_err(|error| map::roku(&error, Capability::PowerState))?;
                *mac = info.mac();
                Ok(mac.clone())
            }
            // `GET /api/v2/` needs no pairing, so this works before the remote
            // channel is ever opened — which is the point: a set that is asleep
            // cannot be paired with, and its address is exactly what waking it needs.
            Self::Tizen { client, mac, .. } => {
                let info = client
                    .device_info()
                    .await
                    .map_err(|error| map::tizen(&error, Capability::Wake))?;
                *mac = info.mac();
                Ok(mac.clone())
            }
            Self::Webos { .. } => Err(TvError::UnsupportedOperation {
                vendor: "lg",
                operation: "learn_mac",
            }),
        }
    }

    /// Drop every pooled connection this handle owns.
    ///
    /// A mobile caller needs this: a backgrounded app must not keep a WebSocket
    /// open to a television, and the OS will not close one for it. The pools are
    /// what hold those sockets, so this is the only way to release them without
    /// dropping the handle. Returns the number of connections dropped — Roku
    /// holds none, because ECP is stateless.
    ///
    /// Calling it twice is not an error: the second call drops nothing and
    /// returns zero.
    pub async fn close_all(&self) -> usize {
        match self {
            Self::Tizen { pool, .. } => pool.clear().await,
            Self::Webos { pool, .. } => pool.clear().await,
            Self::Roku { .. } => 0,
        }
    }
}

impl DeviceHandle for TvClient {
    fn id(&self) -> DeviceId {
        // Host-keyed, unconditionally: `TvClient` never retains a UDN — a
        // handle built from discovery and one built from a bare address derive
        // the same thing here, the host of `address()`. Discovery-time identity
        // (USN → serial → MAC precedence, for fleet and inventory) lives on the
        // record itself, as `DiscoveredTv::stable_key` (crate::types); this id
        // is the connection-scope key — stable exactly as long as the address
        // is (a static-IP install, or a DHCP lease that holds).
        DeviceId::new(host_of(self.address())).unwrap_or_else(|_| DeviceId::anonymous())
    }

    fn descriptor(&self) -> DeviceDescriptor {
        let descriptor =
            DeviceDescriptor::new(self.id(), device_os(self.vendor()), host_of(self.address()));
        match self.mac() {
            Some(mac) => descriptor.with_mac(mac),
            None => descriptor,
        }
    }

    fn capabilities(&self) -> CapabilitySet {
        self.core_capabilities()
    }

    fn as_power(&self) -> Option<&dyn PowerControl> {
        Some(self)
    }

    fn as_text(&self) -> Option<&dyn TextInput> {
        self.core_capabilities()
            .contains(Capability::TextInput)
            .then_some(self as &dyn TextInput)
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
        // webOS gained `switchInput`, so the accessor has to follow the advertised
        // set the way `as_key` and `as_volume` do — the conformance suite checks
        // exactly this pairing, and it caught the mismatch when the capability
        // landed without it.
        self.core_capabilities()
            .contains(Capability::InputSelect)
            .then_some(self as &dyn InputSelect)
    }

    fn as_volume(&self) -> Option<&dyn VolumeControl> {
        self.core_capabilities()
            .contains(Capability::Volume)
            .then_some(self as &dyn VolumeControl)
    }

    fn as_screen_cast(&self) -> Option<&dyn ScreenCastProvider> {
        self.core_capabilities()
            .contains(Capability::ScreenCast)
            .then_some(self as &dyn ScreenCastProvider)
    }
}

#[async_trait]
impl ScreenCastProvider for TvClient {
    async fn start_cast(
        &self,
        _target_url: Option<&str>,
        cmd: tv_rs_core::CommandId,
    ) -> Result<(), TvError> {
        // Wrapped even though cast refuses today: every cmd-carrying mutation
        // goes through the one registry, so a cast that ever reaches the wire
        // is covered without anyone remembering to add the wrapper then.
        dedupe::global()
            .call(host_of(self.address()), cmd, async {
                Err(TvError::Unsupported(Capability::ScreenCast))
            })
            .await
    }

    async fn stop_cast(&self, cmd: tv_rs_core::CommandId) -> Result<(), TvError> {
        dedupe::global()
            .call(host_of(self.address()), cmd, async {
                Err(TvError::Unsupported(Capability::ScreenCast))
            })
            .await
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
        match self {
            // ECP answers while the screen is dark, so this is normally enough. A
            // set that is fully off answers nothing at all, and then the packet is
            // the only path — but only when a caller supplied an address.
            Self::Roku { client, .. } => match client.power_on().await {
                Ok(()) => Ok(()),
                Err(error) => match self.mac() {
                    Some(mac) => wake_with_packet(mac).await,
                    None => Err(map::roku(&error, Capability::Wake)),
                },
            },
            // Tizen and webOS drop their network stack when they sleep, so the
            // broadcast packet is the only path — and it needs the address the
            // caller supplied. Without one this is a refusal, not a guess.
            Self::Tizen { .. } | Self::Webos { .. } => match self.mac() {
                Some(mac) => wake_with_packet(mac).await,
                None => Err(TvError::Unsupported(Capability::Wake)),
            },
        }
    }

    async fn sleep(&self) -> Result<(), TvError> {
        TvClient::power_off(self).await
    }
}

#[async_trait]
impl KeyInput for TvClient {
    async fn key_press(&self, key: Key, cmd: tv_rs_core::CommandId) -> Result<(), TvError> {
        dedupe::global()
            .call(host_of(self.address()), cmd, async move {
                match self {
                    Self::Tizen { client, pool, .. } => {
                        let key = tizen_key(key)?;
                        pool.send_key(client, key)
                            .await
                            .map_err(|error| map::tizen(&error, Capability::Key))
                    }
                    // webOS has no remote-key endpoint: SSAP drives the pointer-input
                    // socket instead, which is not implemented yet (plan item B1). A
                    // refusal is the honest answer — pressing nothing is not a key.
                    Self::Webos { client, pool, .. } => {
                        let name = webos_key(key)?;
                        pool.send_key(client, name)
                            .await
                            .map_err(|error| map::webos(&error, Capability::Key))
                    }
                    Self::Roku { client, .. } => {
                        let key = roku_key(key)?;
                        client
                            .send_keypress(key)
                            .await
                            .map_err(|error| map::roku(&error, Capability::Key))
                    }
                }
            })
            .await
    }
}

#[async_trait]
impl AppLaunch for TvClient {
    async fn launch(&self, app: AppRef, cmd: tv_rs_core::CommandId) -> Result<(), TvError> {
        dedupe::global()
            .call(host_of(self.address()), cmd, async move {
                TvClient::launch_app(self, &app.id).await
            })
            .await
    }

    async fn foreground(&self) -> Result<Option<AppRef>, TvError> {
        match self {
            Self::Webos { client, pool, .. } => {
                let app_id = pool
                    .foreground_app(client)
                    .await
                    .map_err(|error| map::webos(&error, Capability::Foreground))?;
                Ok(app_id.map(AppRef::new))
            }
            // The home screen reports `Roku` with no id, and that is not an
            // application: `None` is the honest answer for "nothing is in front".
            Self::Roku { client, .. } => {
                let active = client
                    .query_active_app()
                    .await
                    .map_err(|error| map::roku(&error, Capability::Foreground))?;
                Ok(active.app_id.map(AppRef::new))
            }
            Self::Tizen { .. } => Err(TvError::Unsupported(Capability::Foreground)),
        }
    }

    /// Tizen launches through DIAL when a payload has to travel with the app id.
    ///
    /// DIAL is the only surface here that carries a body, so a bare launch keeps
    /// the websocket command every other vendor uses — this only takes over when
    /// there is actually something to hand over. `open_url` stays untouched: a set
    /// with no browser has no browser.
    async fn launch_with_payload(
        &self,
        app: AppRef,
        payload: Option<&str>,
        cmd: tv_rs_core::CommandId,
    ) -> Result<(), TvError> {
        // The `None` payload arm funnels into `launch`, which carries its own
        // registry check for the same id — a harmless second look that keeps
        // one deduplication story instead of two.
        dedupe::global()
            .call(host_of(self.address()), cmd, async move {
                match (self, payload) {
                    (Self::Tizen { client, .. }, Some(payload)) => client
                        .dial_launch(&app.id, Some(payload))
                        .await
                        .map_err(|error| map::tizen(&error, Capability::AppLaunch)),
                    (_, Some(_)) => Err(TvError::Unsupported(Capability::AppLaunch)),
                    (_, None) => self.launch(app, cmd).await,
                }
            })
            .await
    }

    /// Lists installed applications. webOS answers `listApps` and Roku answers
    /// `/query/apps`; Tizen exposes neither over its REST API, so it keeps the
    /// trait default rather than a fabricated catalogue.
    async fn installed_apps(&self) -> Result<Vec<AppRef>, TvError> {
        match self {
            Self::Webos { client, pool, .. } => {
                let apps = pool
                    .list_apps(client)
                    .await
                    .map_err(|error| map::webos(&error, Capability::AppLaunch))?;
                Ok(apps
                    .into_iter()
                    .map(|app| AppRef {
                        id: app.id,
                        name: app.title,
                    })
                    .collect())
            }
            Self::Roku { client, .. } => {
                let apps = client
                    .query_apps()
                    .await
                    .map_err(|error| map::roku(&error, Capability::AppLaunch))?;
                Ok(apps
                    .into_iter()
                    .map(|app| AppRef {
                        id: app.id,
                        name: Some(app.name),
                    })
                    .collect())
            }
            // Listing is its own capability, so the refusal names `AppList`: Tizen
            // launches what it cannot enumerate, and saying `AppLaunch` here would
            // claim the wrong thing.
            Self::Tizen { .. } => Err(TvError::Unsupported(Capability::AppList)),
        }
    }
}

#[async_trait]
impl TextInput for TvClient {
    /// Types a string into the focused field.
    ///
    /// Only Roku has a verified surface here (`keypress/Lit_<char>`). webOS's IME
    /// and Tizen's `SendInputString` are named in the plan but are not traced to a
    /// source in this workspace, so both keep the trait default, which names
    /// [`Capability::TextInput`] — an unverified protocol operation does not
    /// exist, and a search box that types nothing is worse than one that refuses.
    async fn type_text(&self, text: &str, cmd: CommandId) -> Result<(), TvError> {
        dedupe::global()
            .call(host_of(self.address()), cmd, async move {
                match self {
                    Self::Roku { client, .. } => client
                        .type_text(text)
                        .await
                        .map_err(|error| map::roku(&error, Capability::TextInput)),
                    // webOS types through its IME service, and the text is plain UTF-8.
                    Self::Webos { client, pool, .. } => pool
                        .insert_text(client, text, false)
                        .await
                        .map_err(|error| map::webos(&error, Capability::TextInput)),
                    // Tizen base64s the string into `Cmd`; Roku sends a key per character and
                    // webOS takes plain UTF-8. Three frames, one method.
                    Self::Tizen { client, pool, .. } => pool
                        .send_text(client, text)
                        .await
                        .map_err(|error| map::tizen(&error, Capability::TextInput)),
                }
            })
            .await
    }
}

#[async_trait]
impl InputSelect for TvClient {
    /// webOS exposes no reliable "which input is live" read, so this is `None`
    /// rather than a guess; the UI shows an unknown input dot.
    async fn active_input(&self) -> Result<Option<Input>, TvError> {
        match self {
            Self::Webos { .. } => Ok(None),
            _ => Err(TvError::Unsupported(Capability::InputSelect)),
        }
    }

    /// webOS switches with `ssap://tv/switchInput`.
    async fn select_input(&self, input: Input, cmd: tv_rs_core::CommandId) -> Result<(), TvError> {
        dedupe::global()
            .call(host_of(self.address()), cmd, async move {
                match self {
                    Self::Webos { client, pool, .. } => {
                        let input_id = webos_input(input)?;
                        pool.switch_input(client, &input_id)
                            .await
                            .map_err(|error| map::webos(&error, Capability::InputSelect))
                    }
                    _ => Err(TvError::Unsupported(Capability::InputSelect)),
                }
            })
            .await
    }
}

#[async_trait]
impl VolumeControl for TvClient {
    async fn volume(&self) -> Result<Option<u8>, TvError> {
        match self {
            Self::Webos { client, pool, .. } => pool
                .get_volume(client)
                .await
                .map(Some)
                .map_err(|error| map::webos(&error, Capability::Volume)),
            _ => Err(TvError::Unsupported(Capability::Volume)),
        }
    }

    /// Reads the mute state. webOS answers `ssap://audio/getMute`; the others
    /// cannot say yet, which is `Ok(None)` rather than an error — a vendor that
    /// can toggle mute but not read it is not a failure.
    async fn mute(&self) -> Result<Option<bool>, TvError> {
        match self {
            Self::Webos { client, pool, .. } => pool
                .get_mute(client)
                .await
                .map(Some)
                .map_err(|error| map::webos(&error, Capability::Volume)),
            _ => Ok(None),
        }
    }

    async fn set_volume(&self, level: u8, cmd: tv_rs_core::CommandId) -> Result<(), TvError> {
        dedupe::global()
            .call(host_of(self.address()), cmd, async move {
                match self {
                    Self::Webos { client, pool, .. } => pool
                        .set_volume(client, level)
                        .await
                        .map_err(|error| map::webos(&error, Capability::Volume)),
                    _ => Err(TvError::Unsupported(Capability::Volume)),
                }
            })
            .await
    }

    async fn set_mute(&self, muted: bool, cmd: tv_rs_core::CommandId) -> Result<(), TvError> {
        dedupe::global()
            .call(host_of(self.address()), cmd, async move {
                match self {
                    Self::Webos { client, pool, .. } => pool
                        .set_mute(client, muted)
                        .await
                        .map_err(|error| map::webos(&error, Capability::Volume)),
                    _ => Err(TvError::Unsupported(Capability::Volume)),
                }
            })
            .await
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
/// Sends the magic packet a sleeping panel needs.
///
/// A packet is addressed to a hardware address, so a malformed one is a
/// `Protocol::Malformed` rather than a silent no-op.
async fn wake_with_packet(mac: &str) -> Result<(), TvError> {
    let address = mac.parse::<MacAddress>().map_err(|_| {
        TvError::Protocol(ProtocolError::Malformed(format!(
            "`{mac}` is not a hardware address"
        )))
    })?;
    tv_rs_power::wake_on_lan(&address)
        .await
        .map_err(|error| map::power(&error))
}

/// Maps a vendor-neutral input to the id webOS' `switchInput` expects.
///
/// The ids are the ones `ssap://tv/getExternalInputList` reports: `HDMI_1`,
/// `HDMI_2`, … (1-based), and `tv` for the tuner. USB is media rather than an
/// input, so it is refused instead of guessed at. Unverified on a real panel —
/// there is no LG set here — so the ids are pinned by tests, not a screenshot.
fn webos_input(input: Input) -> Result<String, TvError> {
    Ok(match input {
        Input::Hdmi(port) if port >= 1 => format!("HDMI_{port}"),
        Input::Tv => "tv".to_owned(),
        Input::Other(name) => name,
        Input::Hdmi(_) | Input::Usb(_) => {
            return Err(TvError::Unsupported(Capability::InputSelect));
        }
    })
}

/// Maps a vendor-neutral key to the webOS pointer channel's button name.
///
/// webOS has no remote-key endpoint: SSAP exposes no `KEY_*` message, so a
/// press is a `type:button` frame on the pointer socket (`tv-rs-webos`
/// `pointer.rs`). The names are the channel's own vocabulary — uppercase, no
/// spaces — and they are case-sensitive on the wire.
///
/// Only the documented subset is mapped. A key the pointer channel has no
/// frame for is `Unsupported`, never a silently dropped press: the caller's
/// capability check and the user's error are both better than a button that
/// does nothing. `Key::Power` is deliberately absent — power goes through
/// `ssap://system/turnOff`, not the pointer channel.
fn webos_key(key: Key) -> Result<&'static str, TvError> {
    /// The pointer channel takes a bare digit for `Key::Digit(n)`.
    const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];

    Ok(match key {
        Key::Up => "UP",
        Key::Down => "DOWN",
        Key::Left => "LEFT",
        Key::Right => "RIGHT",
        Key::Select | Key::Enter => "ENTER",
        Key::Back => "BACK",
        Key::Home => "HOME",
        Key::Menu => "MENU",
        Key::Info => "INFO",
        Key::Play => "PLAY",
        Key::Pause => "PAUSE",
        Key::Stop => "STOP",
        Key::Rewind => "REWIND",
        Key::FastForward => "FASTFORWARD",
        Key::ChannelUp => "CHANNELUP",
        Key::ChannelDown => "CHANNELDOWN",
        Key::VolumeUp => "VOLUMEUP",
        Key::VolumeDown => "VOLUMEDOWN",
        Key::Mute => "MUTE",
        Key::Digit(n @ 0..=9) => DIGITS[n as usize],
        _ => return Err(TvError::Unsupported(Capability::Key)),
    })
}

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
    use std::sync::Arc;

    use super::*;
    use tv_rs_roku::RokuClient;
    use tv_rs_tizen::{DynConnectionPool, LiveChannelFactory, TizenClient};
    use tv_rs_webos::{DynSessionPool, LiveSessionFactory, WebosClient};

    fn tizen() -> TvClient {
        TvClient::Tizen {
            client: TizenClient::new("10.0.0.1"),
            pool: Arc::new(DynConnectionPool::with_factory(Box::new(
                LiveChannelFactory,
            ))),
            mac: None,
        }
    }

    fn webos() -> TvClient {
        TvClient::Webos {
            client: WebosClient::new("10.0.0.3"),
            pool: Arc::new(DynSessionPool::with_factory(Box::new(LiveSessionFactory))),
            mac: None,
        }
    }

    fn roku() -> TvClient {
        TvClient::Roku {
            client: RokuClient::new("10.0.0.4"),
            mac: None,
        }
    }

    /// A payload belongs to DIAL, and only Tizen has DIAL. The others must refuse
    /// rather than launch the app and silently lose the reason it was launched.
    #[tokio::test]
    async fn a_payload_is_refused_where_no_platform_can_carry_it() {
        for client in [webos(), roku()] {
            let error = AppLaunch::launch_with_payload(
                &client,
                AppRef::new("com.example.app"),
                Some("https://example.test/deep"),
                CommandId::new(),
            )
            .await
            .expect_err("no payload surface");
            assert!(
                matches!(error, TvError::Unsupported(Capability::AppLaunch)),
                "{error:?}"
            );
        }
    }

    /// Tizen must reach DIAL rather than answer `Unsupported`: an unreachable
    /// address gives a transport error, which is the proof the request went out.
    #[tokio::test]
    async fn tizen_sends_a_payload_over_dial() {
        let error = AppLaunch::launch_with_payload(
            &tizen(),
            AppRef::new("1110110"),
            Some("https://example.test/deep"),
            CommandId::new(),
        )
        .await
        .expect_err("the test address is not there");
        assert!(!matches!(error, TvError::Unsupported(_)), "{error:?}");
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
                    Capability::TextInput,
                ],
            ),
            (
                webos(),
                vec![
                    Capability::PowerState,
                    Capability::Key,
                    Capability::AppLaunch,
                    Capability::Foreground,
                    Capability::InputSelect,
                    Capability::Volume,
                    Capability::AppList,
                    Capability::TextInput,
                ],
            ),
            (
                roku(),
                vec![
                    Capability::PowerState,
                    Capability::Wake,
                    Capability::Key,
                    Capability::AppLaunch,
                    Capability::Foreground,
                    Capability::AppList,
                    Capability::TextInput,
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
            // Wake is real on Roku (ECP answers while the screen is dark) and on
            // the others only once a caller has supplied the address a magic
            // packet needs — never merely because the trait exists.
            assert_eq!(
                set.contains(Capability::Wake),
                client.mac().is_some() || matches!(client.vendor(), TvVendor::Roku),
                "wake is advertised exactly where it can be delivered"
            );
            assert_eq!(set.contains(Capability::Key), client.as_key().is_some());
            assert!(client.as_app().is_some());
            assert_eq!(
                set.contains(Capability::Volume),
                client.as_volume().is_some()
            );
            // The accessor must follow the advertised set for every capability —
            // the conformance suite checks this pairing, and hardcoding "nobody
            // implements input" is how this assert went stale the moment webOS
            // gained `switchInput`.
            assert_eq!(
                set.contains(Capability::InputSelect),
                client.as_input().is_some(),
                "the input accessor follows the advertised set"
            );
            assert_eq!(
                set.contains(Capability::TextInput),
                client.as_text().is_some(),
                "the text accessor follows the advertised set"
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

    #[test]
    fn a_supplied_mac_is_remembered_and_advertised() {
        let client = tizen().with_mac("aa:bb:cc:dd:ee:ff");
        assert_eq!(client.mac(), Some("aa:bb:cc:dd:ee:ff"));
        assert!(
            client.capabilities().contains(Capability::Wake),
            "a known address is what makes the capability honest"
        );
        assert_eq!(
            client.descriptor().mac.as_deref(),
            Some("aa:bb:cc:dd:ee:ff"),
            "the descriptor is where a fleet reads identity"
        );
    }

    #[tokio::test]
    async fn a_supplied_mac_reaches_the_wake_path() {
        // The packet is broadcast, so this asserts the routing rather than the
        // network: what matters is that a known address is no longer refused.
        let client = tizen().with_mac("aa:bb:cc:dd:ee:ff");
        let result = PowerControl::wake(&client).await;
        assert!(
            !matches!(result, Err(TvError::Unsupported(_))),
            "a known MAC must not be refused: {result:?}"
        );
    }

    #[tokio::test]
    async fn a_malformed_mac_is_reported_rather_than_broadcast() {
        let client = tizen().with_mac("not-a-mac");
        let error = PowerControl::wake(&client).await.expect_err("bad address");
        assert!(matches!(error, TvError::Protocol(_)), "{error:?}");
    }

    #[tokio::test]
    async fn roku_wakes_over_ecp_not_with_a_packet() {
        // Port 1 refuses instantly, so this is fast: the point is that Roku
        // routes to ECP rather than answering `Unsupported`.
        let client = TvClient::Roku {
            client: RokuClient::with_base_url("http://127.0.0.1:1"),
            mac: None,
        };
        let error = PowerControl::wake(&client)
            .await
            .expect_err("nothing listens");
        assert!(!matches!(error, TvError::Unsupported(_)), "{error:?}");
    }

    #[tokio::test]
    async fn roku_foreground_asks_the_device() {
        // Port 1 refuses instantly: what matters is that the call reaches ECP
        // instead of answering `Unsupported`.
        let client = TvClient::Roku {
            client: RokuClient::with_base_url("http://127.0.0.1:1"),
            mac: None,
        };
        let error = AppLaunch::foreground(&client)
            .await
            .expect_err("nothing listens");
        assert!(!matches!(error, TvError::Unsupported(_)), "{error:?}");
    }

    /// The whole point of `AppList` as its own capability: Tizen launches what it
    /// cannot enumerate, so the refusal must name the listing, never the launching.
    #[tokio::test]
    async fn tizen_refuses_to_list_without_refusing_to_launch() {
        let client = tizen();
        assert!(!client.capabilities().contains(Capability::AppList));
        let error = AppLaunch::installed_apps(&client)
            .await
            .expect_err("Tizen keeps the trait default");
        assert!(
            matches!(error, TvError::Unsupported(Capability::AppList)),
            "{error:?}"
        );
        assert!(!matches!(
            error,
            TvError::Unsupported(Capability::AppLaunch)
        ));
    }

    /// Advertised where it is delivered: Roku (`/query/apps`) and webOS
    /// (`listApps`) enumerate, and neither advertises a capability it cannot back.
    #[test]
    fn app_list_is_advertised_where_the_catalogue_is_real() {
        assert!(roku().capabilities().contains(Capability::AppList));
        assert!(webos().capabilities().contains(Capability::AppList));
        assert!(!tizen().capabilities().contains(Capability::AppList));
    }

    /// The arm must be reached, not the trait default: port 1 refuses instantly,
    /// so a transport error here proves the request was sent.
    #[tokio::test]
    async fn installed_apps_reaches_the_device() {
        let client = TvClient::Roku {
            client: RokuClient::with_base_url("http://127.0.0.1:1"),
            mac: None,
        };
        let error = AppLaunch::installed_apps(&client)
            .await
            .expect_err("nothing listens");
        assert!(!matches!(error, TvError::Unsupported(_)), "{error:?}");
    }

    /// Advertised where it is delivered — and all three now deliver it, each through a
    /// different frame: Roku presses a `Lit_<char>` key per character, webOS calls its
    /// IME with plain UTF-8, Tizen base64s the string into a `SendInputString` command.
    #[test]
    fn text_input_is_advertised_where_it_is_delivered() {
        assert!(roku().capabilities().contains(Capability::TextInput));
        assert!(webos().capabilities().contains(Capability::TextInput));
        assert!(tizen().capabilities().contains(Capability::TextInput));
    }

    /// The arm must be reached, not the trait default: port 1 refuses instantly, so
    /// a transport error here proves the request was sent.
    #[tokio::test]
    async fn type_text_reaches_the_device() {
        let client = TvClient::Roku {
            client: RokuClient::with_base_url("http://127.0.0.1:1"),
            mac: None,
        };
        let error = TextInput::type_text(&client, "hello", CommandId::new())
            .await
            .expect_err("nothing listens");
        assert!(!matches!(error, TvError::Unsupported(_)), "{error:?}");
    }

    /// Tizen's call must reach the channel rather than answer `Unsupported`: the
    /// address is unreachable, so a transport error is the proof it was sent.
    #[tokio::test]
    async fn tizen_type_text_reaches_the_device() {
        let error = TextInput::type_text(&tizen(), "hello", CommandId::new())
            .await
            .expect_err("10.0.0.1 is not there");
        assert!(!matches!(error, TvError::Unsupported(_)), "{error:?}");
    }

    /// webOS types through its IME service, so the call must reach the device rather
    /// than answer `Unsupported` — an unreachable address gives a transport error,
    /// which is the proof it was sent.
    #[tokio::test]
    async fn webos_type_text_reaches_the_device() {
        let error = TextInput::type_text(&webos(), "hello", CommandId::new())
            .await
            .expect_err("10.0.0.3 is not there");
        assert!(!matches!(error, TvError::Unsupported(_)), "{error:?}");
    }

    /// The `CommandId` contract at the facade, end to end: two deliveries of
    /// one id must have the effect of one, so the second is answered from the
    /// registry and never reaches the wire. The stub counts what actually
    /// arrives — with no television in the loop, and nothing new linked in.
    #[tokio::test]
    async fn a_replayed_command_id_reaches_the_device_once() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::{TcpListener, TcpStream};

        /// Reads one ECP request head, so the socket can close with a FIN
        /// instead of a reset while the client is still writing into it.
        async fn read_head(stream: &mut TcpStream) {
            let mut head = Vec::new();
            let mut byte = [0u8; 1];
            while head.len() < 8 * 1024 {
                match stream.read(&mut byte).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        head.push(byte[0]);
                        if head.ends_with(b"\r\n\r\n") {
                            break;
                        }
                    }
                }
            }
        }

        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a free port");
        let address = listener.local_addr().expect("bound");
        let hits = Arc::new(AtomicUsize::new(0));
        let done = Arc::new(AtomicBool::new(false));

        let stub_hits = hits.clone();
        let stub_done = done.clone();
        let stub = tokio::spawn(async move {
            loop {
                let Ok((mut stream, _)) = listener.accept().await else {
                    break;
                };
                if stub_done.load(Ordering::SeqCst) {
                    break; // the shutdown probe, not a command
                }
                stub_hits.fetch_add(1, Ordering::SeqCst);
                read_head(&mut stream).await;
                let _ = stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
                    .await;
                let _ = stream.shutdown().await;
            }
        });

        let client = TvClient::Roku {
            client: RokuClient::with_base_url(format!("http://{address}")),
            mac: None,
        };
        let cmd = CommandId::new();
        KeyInput::key_press(&client, Key::Up, cmd)
            .await
            .expect("the stub answers");
        KeyInput::key_press(&client, Key::Up, cmd)
            .await
            .expect("the replay is answered without the wire");

        assert_eq!(
            hits.load(Ordering::SeqCst),
            1,
            "the second delivery of one CommandId must not reach the device"
        );

        done.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(address).await; // wakes the accept loop
        stub.await.expect("the stub stops cleanly");
    }

    #[tokio::test]
    async fn learn_mac_refuses_only_where_the_vendor_offers_nothing() {
        // Samsung reaches `GET /api/v2/`. The address here is unreachable, so a
        // transport error is the proof that the request was sent — what matters is
        // that it is *not* a refusal.
        let mut tizen = tizen();
        let error = tizen.learn_mac().await.expect_err("10.0.0.1 is not there");
        assert!(
            !matches!(error, TvError::UnsupportedOperation { .. }),
            "Samsung has a verified endpoint: {error:?}"
        );

        // webOS refuses, and that is a vendor answer rather than a gap: its
        // `getSystemInfo` key set is closed and carries no MAC, LG staff have said
        // twice that no MAC API is provided, and the one endpoint that does carry one
        // is open-source-only and gated at API level 29.
        let mut webos = webos();
        let error = webos.learn_mac().await.expect_err("no such API");
        assert!(
            matches!(error, TvError::UnsupportedOperation { vendor: "lg", .. }),
            "{error:?}"
        );
    }

    #[test]
    fn webos_maps_keys_to_pointer_button_names() {
        // webOS has no key endpoint: a press is a pointer-channel button frame.
        // These strings are the wire vocabulary, so a typo here is a dead button.
        assert_eq!(webos_key(Key::Home).expect("mapped"), "HOME");
        assert_eq!(webos_key(Key::Select).expect("mapped"), "ENTER");
        assert_eq!(webos_key(Key::Enter).expect("mapped"), "ENTER");
        assert_eq!(webos_key(Key::VolumeUp).expect("mapped"), "VOLUMEUP");
        assert_eq!(webos_key(Key::FastForward).expect("mapped"), "FASTFORWARD");
        assert_eq!(webos_key(Key::Digit(7)).expect("mapped"), "7");
    }

    #[test]
    fn webos_maps_inputs_to_switch_input_ids() {
        // The ids `ssap://tv/getExternalInputList` reports, 1-based.
        assert_eq!(webos_input(Input::Hdmi(1)).expect("mapped"), "HDMI_1");
        assert_eq!(webos_input(Input::Hdmi(4)).expect("mapped"), "HDMI_4");
        assert_eq!(webos_input(Input::Tv).expect("mapped"), "tv");
        assert_eq!(
            webos_input(Input::Other("AV_1".into())).expect("mapped"),
            "AV_1"
        );
    }

    #[test]
    fn webos_refuses_inputs_it_has_no_id_for() {
        // USB is media, not an input, and HDMI ports are 1-based.
        for input in [Input::Hdmi(0), Input::Usb(1)] {
            // `Input` is not `Copy`, so name it before it is consumed.
            let label = format!("{input:?}");
            assert!(
                matches!(
                    webos_input(input),
                    Err(TvError::Unsupported(Capability::InputSelect))
                ),
                "{label} should be refused"
            );
        }
    }

    #[test]
    fn webos_refuses_keys_the_pointer_channel_has_no_frame_for() {
        // An honest `Unsupported` beats a press that silently does nothing.
        for key in [Key::Power, Key::Previous, Key::Next, Key::Digit(10)] {
            assert!(
                matches!(webos_key(key), Err(TvError::Unsupported(Capability::Key))),
                "{key:?} should have no pointer frame"
            );
        }
    }

    #[test]
    fn screen_cast_is_unsupported_by_default_on_tv_clients() {
        for client in [tizen(), webos(), roku()] {
            assert!(!client.capabilities().contains(Capability::ScreenCast));
            assert!(client.as_screen_cast().is_none());
        }
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
            mac: None,
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

    /// Plan §3.2 exit: twenty key presses must share one accepted connection.
    ///
    /// The counting factory comes from `tv-rs-tizen`'s `testkit` feature, so this
    /// proves the *facade* routes through the pool instead of dialling per press —
    /// with no television in the loop.
    #[cfg(feature = "testkit")]
    #[tokio::test]
    async fn twenty_key_presses_share_one_channel() {
        use std::sync::atomic::Ordering;

        use tv_rs_tizen::testkit::{counters, factory};

        let (opens, sent) = counters();
        let client = TvClient::Tizen {
            client: TizenClient::new("10.0.0.1"),
            pool: Arc::new(DynConnectionPool::with_factory(Box::new(factory(
                opens.clone(),
                0,
                sent.clone(),
            )))),
            mac: None,
        };

        for _ in 0..20 {
            KeyInput::key_press(&client, Key::Up, tv_rs_core::CommandId::new())
                .await
                .expect("the fake channel serves");
        }

        assert_eq!(
            opens.load(Ordering::SeqCst),
            1,
            "twenty presses must not open twenty channels"
        );
        assert_eq!(
            sent.load(Ordering::SeqCst),
            20,
            "every press must reach the TV"
        );
    }

    /// The webOS half of the same exit: repeated reads reuse one session.
    #[cfg(feature = "testkit")]
    #[tokio::test]
    async fn webos_volume_reads_share_one_session() {
        use std::sync::atomic::Ordering;

        use tv_rs_webos::testkit::{counters, factory};

        let (opens, served) = counters();
        let client = TvClient::Webos {
            client: WebosClient::new("10.0.0.3"),
            pool: Arc::new(DynSessionPool::with_factory(Box::new(factory(
                opens.clone(),
                0,
                served.clone(),
            )))),
            mac: None,
        };

        for _ in 0..5 {
            assert_eq!(
                VolumeControl::volume(&client)
                    .await
                    .expect("the fake session serves"),
                Some(42)
            );
        }

        assert_eq!(
            opens.load(Ordering::SeqCst),
            1,
            "one session for five reads"
        );
    }

    /// `close_all` is the mobile-backgrounding lever: it must actually drop the
    /// socket, and calling it again must be harmless.
    #[cfg(feature = "testkit")]
    #[tokio::test]
    async fn close_all_drops_the_pooled_connection() {
        use tv_rs_tizen::testkit::{counters, factory};

        let (opens, sent) = counters();
        let client = TvClient::Tizen {
            client: TizenClient::new("10.0.0.1"),
            pool: Arc::new(DynConnectionPool::with_factory(Box::new(factory(
                opens.clone(),
                0,
                sent.clone(),
            )))),
            mac: None,
        };

        KeyInput::key_press(&client, Key::Up, tv_rs_core::CommandId::new())
            .await
            .expect("the fake channel serves");
        assert_eq!(client.close_all().await, 1, "the open channel is dropped");
        assert_eq!(client.close_all().await, 0, "a second call drops nothing");
    }
}
