//! Vendor classification and protocol target identification.

use core::fmt;

/// The television platform or protocol vendor a driver targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum TvVendor {
    /// Samsung Tizen consumer Smart TV.
    Samsung,
    /// LG webOS Smart TV.
    Lg,
    /// Roku OS streaming player or TV.
    Roku,
    /// Philips commercial panel (SICP).
    Philips,
    /// Google Cast or Chromecast.
    GoogleCast,
    /// HDMI Consumer Electronics Control (CEC).
    Cec,
    /// Samsung Multiple Display Control (commercial panels).
    Mdc,
    /// Philips SICP protocol target.
    Sicp,
    /// Sony Bravia IRCC or consumer infrared.
    Ir,
    /// Samsung Tizen protocol target alias.
    Tizen,
    /// LG webOS protocol target alias.
    WebOs,
    /// Discovered, not identified.
    Unknown,
}

impl TvVendor {
    /// A short, stable label for logs and UI.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Samsung => "samsung",
            Self::Lg => "lg",
            Self::Roku => "roku",
            Self::Philips => "philips",
            Self::GoogleCast => "googlecast",
            Self::Cec => "cec",
            Self::Mdc => "mdc",
            Self::Sicp => "sicp",
            Self::Ir => "ir",
            Self::Tizen => "tizen",
            Self::WebOs => "webos",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for TvVendor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_display_and_as_str_match() {
        assert_eq!(TvVendor::Samsung.as_str(), "samsung");
        assert_eq!(TvVendor::Samsung.to_string(), "samsung");
        assert_eq!(TvVendor::Tizen.as_str(), "tizen");
        assert_eq!(TvVendor::Lg.as_str(), "lg");
        assert_eq!(TvVendor::WebOs.as_str(), "webos");
        assert_eq!(TvVendor::Roku.as_str(), "roku");
        assert_eq!(TvVendor::Mdc.as_str(), "mdc");
        assert_eq!(TvVendor::Sicp.as_str(), "sicp");
        assert_eq!(TvVendor::Ir.as_str(), "ir");
        assert_eq!(TvVendor::Cec.as_str(), "cec");
        assert_eq!(TvVendor::GoogleCast.as_str(), "googlecast");
        assert_eq!(TvVendor::Unknown.as_str(), "unknown");
    }
}
