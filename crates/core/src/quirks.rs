//! The quirks database: model- and firmware-specific workarounds, as data.
//!
//! A television's control surface changes with firmware, and the changes are not
//! announced anywhere a client can read. The response is a versioned table keyed by
//! `(vendor, model prefix, firmware prefix)` that a backend consults **before** it
//! tries a port it knows is dead on that device.
//!
//! Keeping this in TOML rather than `match` arms has two consequences that matter:
//! a new quirk is a data change anyone can submit and review, and the table can be
//! shipped newer than the crate.
//!
//! ```no_run
//! use tv_rs_core::quirks::Quirks;
//!
//! let quirks = Quirks::bundled().expect("bundled table parses");
//! if let Some(quirk) = quirks.lookup("samsung", Some("QN90"), Some("T-KTM")) {
//!     println!("{} -> {}", quirk.issue, quirk.action);
//! }
//! ```

use std::fmt;

/// One documented defect and its documented handling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Quirk {
    /// Vendor the row applies to (`samsung`, `lg`, `roku`, …), lowercase.
    pub vendor: String,
    /// Only apply when the model starts with this. `None` matches every model.
    pub model_prefix: Option<String>,
    /// Only apply when the firmware starts with this. `None` matches every version.
    pub firmware_prefix: Option<String>,
    /// Stable identifier of the defect, e.g. `ws_8001_disabled`.
    pub issue: String,
    /// What the client should do instead, e.g. `start_at_8002`.
    pub action: String,
    /// Why this row exists, in one sentence — the part a contributor can check.
    pub note: Option<String>,
}

impl Quirk {
    /// Whether this row applies to a device.
    #[must_use]
    pub fn matches(&self, vendor: &str, model: Option<&str>, firmware: Option<&str>) -> bool {
        if !self.vendor.eq_ignore_ascii_case(vendor) {
            return false;
        }
        if let Some(prefix) = &self.model_prefix {
            match model {
                Some(model) if model.starts_with(prefix.as_str()) => {}
                _ => return false,
            }
        }
        if let Some(prefix) = &self.firmware_prefix {
            match firmware {
                Some(firmware) if firmware.starts_with(prefix.as_str()) => {}
                _ => return false,
            }
        }
        true
    }
}

/// What a backend should do differently, decoded from a row's `action`.
///
/// The `action` strings in the table are a fixed vocabulary rather than prose: a
/// typo would otherwise turn a workaround into a comment that nothing reads. An
/// unrecognised action is preserved as [`QuirkAction::Other`] so the table can gain
/// vocabulary before the code does, and a test asserts that every action shipped in
/// `quirks/default.toml` decodes to a known variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuirkAction {
    /// Skip straight to this port and never fall back: the other one is dead on
    /// this firmware, so probing it costs a timeout per command.
    StartAtPort(u16),
    /// The device will refuse keypresses until a setting is enabled; surface
    /// `Auth(NotPermitted)` rather than retrying.
    ExpectAuthRequired,
    /// Trust the capability flags in the device-info response over the model.
    TrustDeviceInfoFlags,
    /// The user has to change something in the TV's settings menu.
    PromptUser,
    /// Vocabulary the crate does not know yet.
    Other(String),
}

impl QuirkAction {
    /// Decodes one `action` string.
    #[must_use]
    pub fn decode(action: &str) -> Self {
        match action.trim() {
            "start_at_8001" => Self::StartAtPort(8001),
            "start_at_8002" => Self::StartAtPort(8002),
            "start_at_3000" => Self::StartAtPort(3000),
            "start_at_3001" => Self::StartAtPort(3001),
            "auth_not_permitted" => Self::ExpectAuthRequired,
            "read_device_info_flags" => Self::TrustDeviceInfoFlags,
            "point_user_at_settings" => Self::PromptUser,
            other => Self::Other(other.to_string()),
        }
    }

    /// The port this action pins, if any.
    #[must_use]
    pub fn port(&self) -> Option<u16> {
        match self {
            Self::StartAtPort(port) => Some(*port),
            _ => None,
        }
    }
}

impl fmt::Display for QuirkAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StartAtPort(port) => write!(f, "start at {port}"),
            Self::ExpectAuthRequired => f.write_str("expect auth refusal"),
            Self::TrustDeviceInfoFlags => f.write_str("trust device-info flags"),
            Self::PromptUser => f.write_str("prompt the user"),
            Self::Other(action) => write!(f, "{action}"),
        }
    }
}

/// The loaded table.
#[derive(Debug, Clone, Default)]
pub struct Quirks {
    entries: Vec<Quirk>,
}

impl Quirks {
    /// Parses a TOML table.
    ///
    /// # Errors
    ///
    /// Returns [`QuirkError::Parse`] on invalid TOML and
    /// [`QuirkError::MissingField`] when a row lacks `vendor`, `issue` or `action` —
    /// a row without those cannot be acted on, so accepting it would silently
    /// disable the workaround it was written for.
    pub fn parse(source: &str) -> Result<Self, QuirkError> {
        let document: toml::Value = source
            .parse()
            .map_err(|error: toml::de::Error| QuirkError::Parse(error.to_string()))?;

        let rows = document
            .get("quirk")
            .and_then(toml::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        let mut entries = Vec::with_capacity(rows.len());
        for (index, row) in rows.iter().enumerate() {
            let string = |key: &str| -> Option<String> {
                row.get(key)
                    .and_then(toml::Value::as_str)
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
            };
            let required = |key: &'static str| -> Result<String, QuirkError> {
                string(key).ok_or(QuirkError::MissingField {
                    row: index,
                    field: key,
                })
            };

            entries.push(Quirk {
                vendor: required("vendor")?.to_ascii_lowercase(),
                model_prefix: string("model_prefix"),
                firmware_prefix: string("firmware_prefix"),
                issue: required("issue")?,
                action: required("action")?,
                note: string("note"),
            });
        }

        Ok(Self { entries })
    }

    /// Parses the table shipped with this crate (`quirks/default.toml`).
    ///
    /// # Errors
    ///
    /// Returns a parse error only if the bundled file is broken, which the crate's
    /// own tests prevent.
    pub fn bundled() -> Result<Self, QuirkError> {
        Self::parse(include_str!("../../../quirks/default.toml"))
    }

    /// The rows, in file order.
    #[must_use]
    pub fn entries(&self) -> &[Quirk] {
        &self.entries
    }

    /// The first row that applies, or `None`.
    ///
    /// First match wins, so specific rows must be listed before general ones — the
    /// order in the file is the precedence order, and that is stated in the file.
    #[must_use]
    pub fn lookup(
        &self,
        vendor: &str,
        model: Option<&str>,
        firmware: Option<&str>,
    ) -> Option<&Quirk> {
        self.entries
            .iter()
            .find(|quirk| quirk.matches(vendor, model, firmware))
    }

    /// The decoded action for a device, or `None` when no row applies.
    ///
    /// This is the function a backend calls: [`Quirks::lookup`] returns the row for
    /// diagnostics, this returns the thing to *do*.
    #[must_use]
    pub fn action_for(
        &self,
        vendor: &str,
        model: Option<&str>,
        firmware: Option<&str>,
    ) -> Option<QuirkAction> {
        self.lookup(vendor, model, firmware)
            .map(|quirk| QuirkAction::decode(&quirk.action))
    }
}

/// Why a table could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum QuirkError {
    /// The TOML did not parse.
    #[error("invalid quirks table: {0}")]
    Parse(String),
    /// A row was missing a required field.
    #[error("quirk row {row} is missing '{field}'")]
    MissingField {
        /// Zero-based row index in the file.
        row: usize,
        /// The missing field.
        field: &'static str,
    },
}

impl fmt::Display for Quirk {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {} -> {}", self.vendor, self.issue, self.action)
    }
}

#[cfg(test)]
mod tests {
    use super::{QuirkAction, Quirks};

    #[test]
    fn the_bundled_table_parses_and_is_not_empty() {
        let quirks = Quirks::bundled().expect("bundled table");
        assert!(
            quirks.entries().len() >= 5,
            "the shipped table is the point"
        );
    }

    #[test]
    fn every_shipped_action_decodes_to_known_vocabulary() {
        // A typo in the table would otherwise turn a workaround into a comment.
        let quirks = Quirks::bundled().expect("bundled table");
        for quirk in quirks.entries() {
            let action = QuirkAction::decode(&quirk.action);
            assert!(
                !matches!(action, QuirkAction::Other(_)),
                "{} ships an action this crate cannot act on: '{}'",
                quirk.issue,
                quirk.action
            );
        }
    }

    #[test]
    fn a_matching_row_becomes_an_action_a_backend_can_use() {
        let quirks = Quirks::bundled().expect("bundled table");

        // 2020+ Neo QLED: 8001 is dead, so the ladder must not probe it.
        assert_eq!(
            quirks.action_for("samsung", Some("QN90A"), Some("T-KTM")),
            Some(QuirkAction::StartAtPort(8002))
        );
        // Older firmware: still TLS-first, same instruction.
        assert_eq!(
            quirks.action_for("samsung", None, Some("T-KT")),
            Some(QuirkAction::StartAtPort(8002))
        );
        // webOS 23 drops the plaintext port.
        assert_eq!(
            quirks.action_for("lg", None, Some("23")),
            Some(QuirkAction::StartAtPort(3001))
        );
        // No firmware known: the general LG row applies, which asks the user to
        // change a setting rather than silently doing nothing.
        assert_eq!(
            quirks.action_for("lg", None, None),
            Some(QuirkAction::PromptUser)
        );
        // A vendor with no row is not an error.
        assert_eq!(quirks.action_for("sony", None, None), None);
    }

    #[test]
    fn an_unknown_action_is_preserved_rather_than_dropped() {
        assert_eq!(
            QuirkAction::decode("some_future_action"),
            QuirkAction::Other("some_future_action".to_string())
        );
    }

    #[test]
    fn file_order_is_the_precedence_order() {
        let quirks = Quirks::parse(
            r#"
            [[quirk]]
            vendor = "samsung"
            model_prefix = "QN90"
            issue = "specific"
            action = "start_at_8002"

            [[quirk]]
            vendor = "samsung"
            issue = "generic"
            action = "start_at_8001"
            "#,
        )
        .expect("parses");

        assert_eq!(
            quirks
                .lookup("Samsung", Some("QN90DA"), None)
                .map(|q| q.issue.as_str()),
            Some("specific"),
            "vendor matching is case-insensitive and the first match wins"
        );

        // Reversed order: the general row shadows the specific one. That is the
        // documented contract for the file, and this test pins it so a careless
        // append cannot silently disable a workaround.
        let reversed = Quirks::parse(
            r#"
            [[quirk]]
            vendor = "samsung"
            issue = "generic"
            action = "start_at_8001"

            [[quirk]]
            vendor = "samsung"
            model_prefix = "QN90"
            issue = "specific"
            action = "start_at_8002"
            "#,
        )
        .expect("parses");
        assert_eq!(
            reversed
                .lookup("samsung", Some("QN90DA"), None)
                .map(|q| q.issue.as_str()),
            Some("generic")
        );
    }

    #[test]
    fn rows_that_cannot_be_acted_on_are_rejected() {
        let error = Quirks::parse("[[quirk]]\nvendor = \"samsung\"\n").expect_err("no issue");
        assert!(error.to_string().contains("issue"), "{error}");
    }

    #[test]
    fn unknown_vendors_do_not_match() {
        let quirks = Quirks::bundled().expect("bundled table");
        assert!(quirks.lookup("vizio", None, None).is_none());
    }
}
