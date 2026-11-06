//! Capability matrix for the companion app and host layers.
//!
//! Provides hierarchical dotted capability identifiers (`tv.*`, `host.*`, `phone.*`),
//! a unioned capability matrix, dynamic `Hello` state negotiation, and UI tile projection
//! with permission gating (A5/C1 `Blocked`/`RequestPermission`, Android 16 `ACCESS_LOCAL_NETWORK`,
//! and Multicast Lock requirements).

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use crate::descriptor::PowerState;
use crate::error::{Remedy, TvError};

/// Dotted capability identifier representing an atomic capability across endpoints.
///
/// Hierarchical namespaces:
/// - `tv.*`: Capabilities provided or supported by the target TV/display (e.g., `tv.power`, `tv.input.keys`, `tv.app.launch`)
/// - `host.*`: Capabilities provided by the edge signage host or workstation (e.g., `host.uinput`, `host.serial`, `host.ddcci`)
/// - `phone.*`: Capabilities provided by the mobile companion app (e.g., `phone.multicast_lock`, `phone.lan_permission`, `phone.pairing`)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapabilityId(String);

impl CapabilityId {
    /// Creates a new capability ID from a dotted namespace string.
    pub fn new(id: impl Into<String>) -> Result<Self, TvError> {
        let s = id.into();
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(TvError::protocol("capability ID must not be empty"));
        }
        if !trimmed.contains('.') {
            return Err(TvError::protocol(format!(
                "capability ID '{trimmed}' must be a dotted identifier (e.g. 'tv.power', 'host.uinput', 'phone.multicast_lock')"
            )));
        }
        Ok(Self(trimmed.to_string()))
    }

    /// Access the raw string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Returns the top-level domain namespace (e.g., `"tv"`, `"host"`, `"phone"`).
    #[must_use]
    pub fn domain(&self) -> &str {
        self.0.split('.').next().unwrap_or("")
    }

    /// Common TV capability: power control.
    pub const TV_POWER: &'static str = "tv.power";
    /// Common TV capability: key / remote input.
    pub const TV_INPUT_KEYS: &'static str = "tv.input.keys";
    /// Common TV capability: text input / keyboard.
    pub const TV_INPUT_TEXT: &'static str = "tv.input.text";
    /// Common TV capability: application launch.
    pub const TV_APP_LAUNCH: &'static str = "tv.app.launch";
    /// Common TV capability: volume control.
    pub const TV_VOLUME: &'static str = "tv.volume";
    /// Common TV capability: input source selection.
    pub const TV_INPUT_SELECT: &'static str = "tv.input.select";

    /// Host capability: Linux uinput virtual input subsystem or desktop input injection.
    pub const HOST_UINPUT: &'static str = "host.uinput";
    /// Host capability: DDC/CI monitor control.
    pub const HOST_DDCCI: &'static str = "host.ddcci";
    /// Host capability: HDMI-CEC adapter.
    pub const HOST_CEC: &'static str = "host.cec";
    /// Host capability: Serial port access.
    pub const HOST_SERIAL: &'static str = "host.serial";

    /// Phone capability: Android Wi-Fi multicast lock for SSDP/mDNS.
    pub const PHONE_MULTICAST_LOCK: &'static str = "phone.multicast_lock";
    /// Phone capability: Local network access permission (Android 16+ `ACCESS_LOCAL_NETWORK` or iOS Multicast).
    pub const PHONE_LOCAL_NETWORK: &'static str = "phone.local_network";
    /// Phone capability: Active pairing mode / session.
    pub const PHONE_PAIRING: &'static str = "phone.pairing";
}

impl fmt::Display for CapabilityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for CapabilityId {
    type Err = TvError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for CapabilityId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for CapabilityId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::new(s).map_err(serde::de::Error::custom)
    }
}

/// Status of an OS or runtime hardware permission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PermissionStatus {
    /// Permission is granted and active.
    Granted,
    /// Permission has not yet been requested.
    #[default]
    PromptNeeded,
    /// Permission has been denied by user or policy.
    Denied,
    /// Permission is restricted / blocked by platform or missing prerequisite.
    Blocked,
}

/// Diagnostic state when Wi-Fi multicast lock is absent during pairing or discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MulticastLockRequired {
    /// Human-readable explanation.
    pub reason: String,
    /// One-tap action descriptor for the UI.
    pub one_tap_action: String,
}

impl MulticastLockRequired {
    /// Creates a new `MulticastLockRequired` diagnosis with a default one-tap remedy.
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            one_tap_action: "acquire_multicast_lock".to_string(),
        }
    }

    /// Return user-actionable remedy hint.
    #[must_use]
    pub fn hint(&self) -> String {
        format!(
            "Multicast packet reception requires a held Wi-Fi MulticastLock: {}. Tap '{}' to enable.",
            self.reason, self.one_tap_action
        )
    }

    /// Maps to the core `Remedy` taxonomy.
    #[must_use]
    pub fn remedy(&self) -> Remedy {
        Remedy::RequestPermission
    }
}

impl fmt::Display for MulticastLockRequired {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MulticastLockRequired: {}", self.reason)
    }
}

/// Complete capability set of an endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct EndpointCapabilities {
    /// Set of granted/supported capability IDs.
    pub capabilities: BTreeSet<CapabilityId>,
    /// Permissions and their current runtime statuses.
    pub permissions: Vec<(String, PermissionStatus)>,
    /// Additional feature addons or extension tags.
    pub addons: BTreeSet<String>,
}

impl EndpointCapabilities {
    /// Create empty capability set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a dotted capability identifier.
    pub fn with_capability(mut self, cap: impl Into<String>) -> Self {
        if let Ok(id) = CapabilityId::new(cap) {
            self.capabilities.insert(id);
        }
        self
    }

    /// Add a permission status.
    pub fn with_permission(
        mut self,
        permission: impl Into<String>,
        status: PermissionStatus,
    ) -> Self {
        self.permissions.push((permission.into(), status));
        self
    }

    /// Add an addon tag.
    pub fn with_addon(mut self, addon: impl Into<String>) -> Self {
        self.addons.insert(addon.into());
        self
    }

    /// Returns whether this endpoint contains a specific capability ID.
    #[must_use]
    pub fn contains(&self, cap: &str) -> bool {
        self.capabilities.iter().any(|c| c.as_str() == cap)
    }
}

/// Union of capability sets across the TV target, the edge host, and the mobile companion phone.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CapabilityMatrix {
    /// Capabilities of the targeted TV/display endpoint.
    pub tv: EndpointCapabilities,
    /// Capabilities of the edge host machine (if mediating control).
    pub host: EndpointCapabilities,
    /// Capabilities of the companion phone.
    pub phone: EndpointCapabilities,
    /// Current power state of the TV.
    pub power_state: Option<PowerState>,
}

impl CapabilityMatrix {
    /// Create a new empty matrix.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Merge and calculate all effective active capability IDs across all three domains.
    #[must_use]
    pub fn unioned_capabilities(&self) -> BTreeSet<CapabilityId> {
        let mut all = BTreeSet::new();
        all.extend(self.tv.capabilities.iter().cloned());
        all.extend(self.host.capabilities.iter().cloned());
        all.extend(self.phone.capabilities.iter().cloned());
        all
    }

    /// Check if the phone in pairing mode has the necessary multicast lock for discovery/pairing.
    /// Returns `Err(MulticastLockRequired)` if pairing mode is active on phone without the lock.
    pub fn check_phone_pairing_multicast(&self) -> Result<(), MulticastLockRequired> {
        let pairing_active = self.phone.contains(CapabilityId::PHONE_PAIRING)
            || self.phone.addons.contains("pairing_mode");

        if pairing_active && !self.phone.contains(CapabilityId::PHONE_MULTICAST_LOCK) {
            return Err(MulticastLockRequired::new(
                "Phone is in pairing mode but Wi-Fi MulticastLock is not acquired (SSDP/mDNS beacons blocked)",
            ));
        }

        Ok(())
    }

    /// Project the unioned capability matrix into visible and active UI tiles.
    #[must_use]
    pub fn project_ui_tiles(&self) -> BTreeSet<UiTile> {
        let mut tiles = BTreeSet::new();

        // 1. Power tile: visible if TV supports power control
        if self.tv.contains(CapabilityId::TV_POWER) {
            tiles.insert(UiTile::Power);
        }

        // 2. Remote / Keypad tile:
        // Needs remote/key input capability.
        // If mediated through host, host MUST have input injection (e.g. host.uinput)
        // unless TV supports direct remote keys over network.
        // If TV has no direct input capability (e.g. Roku display-only or no input channel),
        // and host has no uinput injection, the Remote tile is NOT shown.
        let tv_has_input = self.tv.contains(CapabilityId::TV_INPUT_KEYS);
        let host_has_uinput = self.host.contains(CapabilityId::HOST_UINPUT);

        // When a host is present in the matrix (has any capabilities or addons),
        // it mediates input, so input must be supported either directly by TV or via host uinput.
        let host_is_present = !self.host.capabilities.is_empty() || !self.host.addons.is_empty();
        let remote_possible = if host_is_present {
            tv_has_input || host_has_uinput
        } else {
            tv_has_input
        };

        if remote_possible {
            tiles.insert(UiTile::Remote);
        }

        // 3. Volume tile
        if self.tv.contains(CapabilityId::TV_VOLUME) {
            tiles.insert(UiTile::Volume);
        }

        // 4. App Launcher tile
        if self.tv.contains(CapabilityId::TV_APP_LAUNCH) {
            tiles.insert(UiTile::AppLauncher);
        }

        // 5. Input Source tile
        if self.tv.contains(CapabilityId::TV_INPUT_SELECT) {
            tiles.insert(UiTile::InputSource);
        }

        // 6. Pairing tile (shown when phone supports pairing or is in pairing mode)
        if self.phone.contains(CapabilityId::PHONE_PAIRING)
            || self.phone.addons.contains("pairing_mode")
        {
            tiles.insert(UiTile::Pairing);
        }

        tiles
    }
}

/// High-level UI tiles presented in the companion application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum UiTile {
    /// Remote control and navigation tile.
    Remote,
    /// Power toggle and status tile.
    Power,
    /// Volume up/down/mute tile.
    Volume,
    /// Application grid / launcher tile.
    AppLauncher,
    /// Physical input switcher (HDMI, etc.).
    InputSource,
    /// Device discovery and pairing setup tile.
    Pairing,
}

impl UiTile {
    /// Return the string identifier for the tile.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Remote => "remote",
            Self::Power => "power",
            Self::Volume => "volume",
            Self::AppLauncher => "app_launcher",
            Self::InputSource => "input_source",
            Self::Pairing => "pairing",
        }
    }
}

/// Update payload emitted on power-level, permission, or add-on changes.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HelloUpdate {
    /// Updated power level or state, if changed.
    pub power_state: Option<PowerState>,
    /// Changed permission states (`(permission_name, status)`).
    pub permissions_delta: Vec<(String, PermissionStatus)>,
    /// Addons added or removed.
    pub addons_delta: Vec<(String, bool)>,
    /// Full updated capability matrix.
    pub matrix: CapabilityMatrix,
}

impl HelloUpdate {
    /// Creates a new `HelloUpdate` reflecting matrix changes.
    #[must_use]
    pub fn new(matrix: CapabilityMatrix) -> Self {
        Self {
            power_state: matrix.power_state,
            permissions_delta: Vec::new(),
            addons_delta: Vec::new(),
            matrix,
        }
    }

    /// Builder method to record a permission change delta.
    #[must_use]
    pub fn with_permission_delta(
        mut self,
        permission: impl Into<String>,
        status: PermissionStatus,
    ) -> Self {
        self.permissions_delta.push((permission.into(), status));
        self
    }

    /// Builder method to record an addon change delta (`(addon, added_or_removed)`).
    #[must_use]
    pub fn with_addon_delta(mut self, addon: impl Into<String>, added: bool) -> Self {
        self.addons_delta.push((addon.into(), added));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_capability_id_validation() {
        assert!(CapabilityId::new("tv.power").is_ok());
        assert!(CapabilityId::new("host.uinput").is_ok());
        assert!(CapabilityId::new("phone.multicast_lock").is_ok());

        assert!(CapabilityId::new("").is_err());
        assert!(CapabilityId::new("nodots").is_err());
        assert_eq!(CapabilityId::new("tv.volume").unwrap().domain(), "tv");
    }

    #[test]
    fn matrix_unions_capabilities_across_endpoints() {
        let mut matrix = CapabilityMatrix::new();
        matrix.tv = EndpointCapabilities::new().with_capability(CapabilityId::TV_POWER);
        matrix.host = EndpointCapabilities::new().with_capability(CapabilityId::HOST_SERIAL);
        matrix.phone =
            EndpointCapabilities::new().with_capability(CapabilityId::PHONE_MULTICAST_LOCK);

        let union = matrix.unioned_capabilities();
        assert_eq!(union.len(), 3);
        assert!(union.contains(&CapabilityId::new(CapabilityId::TV_POWER).unwrap()));
        assert!(union.contains(&CapabilityId::new(CapabilityId::HOST_SERIAL).unwrap()));
        assert!(union.contains(&CapabilityId::new(CapabilityId::PHONE_MULTICAST_LOCK).unwrap()));
    }

    #[test]
    fn exit_criterion_roku_no_input_linux_host_no_uinput_shows_no_remote_tile() {
        // Roku (no input) through Linux host (no uinput) -> phone shows no Remote tile
        let mut matrix = CapabilityMatrix::new();
        // TV has power and volume, but NO input keys:
        matrix.tv = EndpointCapabilities::new()
            .with_capability(CapabilityId::TV_POWER)
            .with_capability(CapabilityId::TV_VOLUME);

        // Host is Linux, has serial/ddcci but NO uinput:
        matrix.host = EndpointCapabilities::new()
            .with_capability(CapabilityId::HOST_SERIAL)
            .with_capability(CapabilityId::HOST_DDCCI);

        let tiles = matrix.project_ui_tiles();
        assert!(tiles.contains(&UiTile::Power));
        assert!(tiles.contains(&UiTile::Volume));
        assert!(
            !tiles.contains(&UiTile::Remote),
            "Remote tile must NOT be visible when TV has no input and host has no uinput"
        );

        // Conversely, if host gains uinput, Remote tile becomes visible:
        matrix.host = matrix.host.with_capability(CapabilityId::HOST_UINPUT);
        let tiles_with_uinput = matrix.project_ui_tiles();
        assert!(tiles_with_uinput.contains(&UiTile::Remote));
    }

    #[test]
    fn exit_criterion_phone_in_pairing_mode_without_lock_returns_multicast_lock_required() {
        // phone in pairing mode without lock -> typed MulticastLockRequired with one-tap fix
        let mut matrix = CapabilityMatrix::new();
        matrix.phone = EndpointCapabilities::new().with_capability(CapabilityId::PHONE_PAIRING);

        // Without multicast lock:
        let err = matrix
            .check_phone_pairing_multicast()
            .expect_err("Must require multicast lock in pairing mode");
        assert_eq!(err.one_tap_action, "acquire_multicast_lock");
        assert_eq!(err.remedy(), Remedy::RequestPermission);
        let hint = err.hint();
        assert!(hint.contains("MulticastLock"), "{hint}");
        assert!(hint.contains("acquire_multicast_lock"), "{hint}");

        // Now with multicast lock:
        matrix.phone = matrix
            .phone
            .with_capability(CapabilityId::PHONE_MULTICAST_LOCK);
        assert!(matrix.check_phone_pairing_multicast().is_ok());
    }

    #[test]
    fn hello_update_delta_tracking() {
        let mut matrix = CapabilityMatrix::new();
        matrix.power_state = Some(PowerState::Standby);
        let update = HelloUpdate::new(matrix)
            .with_permission_delta("ACCESS_LOCAL_NETWORK", PermissionStatus::Granted)
            .with_addon_delta("hdmi_switch", true);

        assert_eq!(update.power_state, Some(PowerState::Standby));
        assert_eq!(update.permissions_delta.len(), 1);
        assert_eq!(update.permissions_delta[0].0, "ACCESS_LOCAL_NETWORK");
        assert_eq!(update.permissions_delta[0].1, PermissionStatus::Granted);
        assert_eq!(update.addons_delta.len(), 1);
        assert_eq!(update.addons_delta[0].0, "hdmi_switch");
        assert!(update.addons_delta[0].1);
    }
}
