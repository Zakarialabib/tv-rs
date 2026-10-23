//! Serde support for the core device model.
//!
//! Enabled with the `serde` feature. The wire shape is the contract between this
//! crate and the companion's `types.ts`:
//!
//! - every capability and key becomes its snake_case label (`"power_state"`,
//!   `"digit_7"`);
//! - [`ObservedField`] becomes
//!   `{"verified_at_ms": u64, "age_ms": u64, "confidence": "high"|"inferred"|"unknown",
//!   "value": T}`;
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
//! assert!(json.contains("\"verified_at_ms\""));
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

use serde::{Deserialize, Serializer};

use crate::capability::Key;
use crate::state::{Confidence, ObservedField};

impl Serialize for Key {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Key::Digit(n) => format!("digit_{}", n),
            other => other.as_str(),
        })
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
        let verified_at_ms = self
            .last_verified_at
            .duration_since(Instant::UNIX_EPOCH)
            .as_millis()
            .try_into()
            .map_err(|_| serde::ser::Error::custom("verified_at_ms overflow"))?;
        let age_ms = self
            .last_verified_at
            .elapsed()
            .as_millis()
            .try_into()
            .map_err(|_| serde::ser::Error::custom("age_ms overflow"))?;
        let mut map = serializer.serialize_map(Some(4))?;
        map.serialize_entry("verified_at_ms", &verified_at_ms)?;
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
        serializer.serialize_str(match self {
            Self::Hdmi(port) => format!("hdmi{}", port),
            Self::Tv => "tv".to_string(),
            Self::Usb(port) => format!("usb{}", port),
            Self::Other(name) => name.clone(),
        })
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
#[must_use]
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
#[must_use]
pub fn from_str<T>(text: &str) -> Result<T, serde_json::Error>
where
    T: Deserialize<'static>,
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
    use crate::capability::{Input, Key};

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
        assert!(json.contains("\"verified_at_ms\""));
        assert!(json.contains("\"confidence\""));
        assert!(json.contains("\"value\""));
        assert!(json.contains("1"));
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
