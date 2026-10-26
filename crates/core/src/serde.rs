//! Serde support for the core device model.
//!
//! Enabled with the `serde` feature. The wire shape is the contract between this
//! crate and the companion's `types.ts`:
//!
//! - every capability and key becomes its snake_case label (`"power_state"`,
//!   `"digit_7"`);
//! - [`ObservedField`] becomes
//!   `{"age_ms": u64, "confidence": "high"|"inferred"|"unknown", "value": T}`. There
//!   is no `verified_at_ms`: `Instant` has no epoch, so only the age is derivable;
//! - every other public data type derives `Serialize`/`Deserialize` with
//!   `rename_all = "snake_case"`.
//!
//! # Examples
//!
//! ```no_run
//! use tv_rs_core::serde;
//! use tv_rs_core::{Key, ObservedField};
//!
//! // A key serialises as the wire string it represents.
//! assert_eq!(serde::to_string(&Key::Digit(7)).unwrap(), "\"digit_7\"");
//! assert_eq!(serde::to_string(&Key::Up).unwrap(), "\"up\"");
//!
//! // An observed field carries its age and confidence.
//! let field = ObservedField::from_read(1u8);
//! let json = serde::to_string(&field).unwrap();
//! assert!(json.contains("\"age_ms\""));
//! assert!(json.contains("\"value\""));
//! ```
//!
//! # How to add a type
//!
//! 1. Put `#[derive(Serialize, Deserialize)]` on the type (with
//!    `rename_all = "snake_case"` for enums). The types in this crate come from
//!    `capability.rs`, `descriptor.rs`, `error.rs`, `state.rs` — not from this file.
//! 2. Override the wire shape of `Key` and `ObservedField` above.
//! 3. Add a test case here.

use std::time::Instant;

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

use crate::capability::{Input, Key};
use crate::state::{ObservedField, ObservedState};

impl Serialize for Key {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let name = match self {
            Key::Digit(n) => format!("digit_{n}"),
            other => other.as_str().to_owned(),
        };
        serializer.serialize_str(&name)
    }
}

impl<T> Serialize for ObservedField<T>
where
    T: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // `Instant` is monotonic and has no epoch, so an absolute
        // `verified_at_ms` is not derivable from it — only the age is. A caller
        // that needs wall-clock time converts the age against its own clock.
        let age_ms = age_ms(&self.last_verified_at);
        let mut map = serializer.serialize_map(Some(3))?;
        map.serialize_entry("age_ms", &age_ms)?;
        map.serialize_entry("confidence", &self.confidence.as_str())?;
        map.serialize_entry("value", &self.value)?;
        map.end()
    }
}

/// An input, serialised as its label: `Hdmi(2)` → `"hdmi2"`, `Tv` → `"tv"`.
impl Serialize for Input {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let label = match self {
            Self::Hdmi(port) => format!("hdmi{port}"),
            Self::Tv => "tv".to_owned(),
            Self::Usb(port) => format!("usb{port}"),
            Self::Other(name) => name.clone(),
        };
        serializer.serialize_str(&label)
    }
}

/// The fleet's per-device state, a map from capability to observed value.
///
/// Derived so long as `ObservedField` is serializable; no custom shape.
impl Serialize for ObservedState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(4))?;
        map.serialize_entry("power", &self.power)?;
        map.serialize_entry("input", &self.input)?;
        map.serialize_entry("foreground", &self.foreground)?;
        map.serialize_entry("volume", &self.volume)?;
        map.end()
    }
}

/// Serialise `value` to a JSON string.
pub fn to_string<T>(value: &T) -> Result<String, serde_json::Error>
where
    T: Serialize,
{
    serde_json::to_string(value)
}

/// Serialise `value` as a JSON `Value`.
pub fn to_value<T>(value: &T) -> Result<serde_json::Value, serde_json::Error>
where
    T: Serialize,
{
    serde_json::to_value(value)
}

/// Parse `text` as `T` from JSON.
pub fn from_str<T>(text: &str) -> Result<T, serde_json::Error>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_str(text)
}

/// Age (in ms) of an [`Instant`] as of now, for the wire's `age_ms` field.
#[must_use]
pub fn age_ms(instant: &Instant) -> u64 {
    instant.elapsed().as_millis().try_into().unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::{Capability, Input, Key};
    use crate::descriptor::PowerState;
    use crate::error::Remedy;
    use crate::state::Confidence;

    #[test]
    fn digit_key_serialises_as_a_wire_string() {
        assert_eq!(
            serde_json::to_string(&Key::Digit(7)).unwrap(),
            "\"digit_7\""
        );
        assert_eq!(serde_json::to_string(&Key::Up).unwrap(), "\"up\"");
    }

    #[test]
    fn input_serialises_as_a_wire_string() {
        assert_eq!(serde_json::to_string(&Input::Hdmi(2)).unwrap(), "\"hdmi2\"");
        assert_eq!(serde_json::to_string(&Input::Tv).unwrap(), "\"tv\"");
        assert_eq!(
            serde_json::to_string(&Input::Other("AV".into())).unwrap(),
            "\"AV\""
        );
    }

    #[test]
    fn observed_field_serialises_with_age_and_confidence() {
        let field = ObservedField::from_read(1u8);
        let json = serde_json::to_string(&field).unwrap();
        assert!(json.contains("\"age_ms\""));
        assert!(json.contains("\"confidence\""));
        assert!(json.contains("\"value\""));
        assert!(json.contains("1"));
    }

    /// Every capability serialises to exactly the label `as_str()` returns.
    ///
    /// `as_str()` is what `types.ts` declares as its capability union, so a drift
    /// between the derive and the label is a broken contract. Adding a variant
    /// here is the only way this list changes.
    #[test]
    fn capability_labels_match_as_str() {
        let all = [
            Capability::PowerState,
            Capability::Wake,
            Capability::Key,
            Capability::AppLaunch,
            Capability::Foreground,
            Capability::InputSelect,
            Capability::Volume,
            Capability::Brightness,
            Capability::Reboot,
            Capability::Telemetry,
            Capability::PanelLock,
            Capability::Orientation,
            Capability::NoSignalStatus,
            Capability::ScreenCast,
        ];
        for capability in all {
            assert_eq!(
                to_string(&capability).expect("serialises"),
                format!("\"{}\"", capability.as_str()),
                "{capability:?} serialises to something other than its label"
            );
        }
    }

    /// The two panel capabilities only exist behind the `supporting` feature,
    /// but they must serialise to their labels too when they are present.
    #[cfg(feature = "supporting")]
    #[test]
    fn supporting_capability_labels_match_as_str() {
        for capability in [Capability::LampOrPanelHours, Capability::ScreenCapture] {
            assert_eq!(
                to_string(&capability).expect("serialises"),
                format!("\"{}\"", capability.as_str())
            );
        }
    }

    /// `Remedy` and the power/confidence vocabulary are the other plain string
    /// unions `types.ts` declares. Note there is no `RequestPermission` here: the
    /// companion app declares that one of its own for the Android LAN block.
    #[test]
    fn vocabulary_is_snake_case() {
        assert_eq!(to_string(&Remedy::Retry).expect("serialises"), "\"retry\"");
        assert_eq!(
            to_string(&Remedy::BackOff).expect("serialises"),
            "\"back_off\""
        );
        assert_eq!(
            to_string(&Remedy::ApplyQuirk).expect("serialises"),
            "\"apply_quirk\""
        );
        assert_eq!(
            to_string(&PowerState::Standby).expect("serialises"),
            "\"standby\""
        );
        assert_eq!(
            to_string(&Confidence::High).expect("serialises"),
            "\"high\""
        );
    }

    #[test]
    fn observed_state_serialises_its_four_fields() {
        let state = ObservedState {
            power: ObservedField::unknown(),
            input: ObservedField::unknown(),
            foreground: ObservedField::unknown(),
            volume: ObservedField::unknown(),
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"power\""));
        assert!(json.contains("\"input\""));
        assert!(json.contains("\"foreground\""));
        assert!(json.contains("\"volume\""));
    }
}
