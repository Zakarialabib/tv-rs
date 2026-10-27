//! Media Path selection and optimal delivery routing.
//!
//! Evaluates candidate media paths (Cast, HDMI source, local player, USB fallback,
//! DLNA renderer) based on device capabilities, signal status, network latency, and format support.

use serde::{Deserialize, Serialize};

/// Discriminant/Type of media path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaPathType {
    /// Casting protocol (e.g., Chromecast / ScreenCast).
    Cast,
    /// Direct HDMI hardware source input.
    HdmiSource,
    /// On-device native local media player.
    LocalPlayer,
    /// USB external storage fallback.
    UsbFallback,
    /// DLNA/UPnP AVTransport MediaRenderer.
    DlnaRenderer,
}

impl MediaPathType {
    /// Returns default priority rank (lower number = higher priority).
    pub fn default_priority(&self) -> u32 {
        match self {
            Self::LocalPlayer => 1,
            Self::HdmiSource => 2,
            Self::DlnaRenderer => 3,
            Self::Cast => 4,
            Self::UsbFallback => 5,
        }
    }
}

/// A specific candidate media delivery path with its target details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "details", rename_all = "snake_case")]
pub enum MediaPath {
    /// Screen casting or stream receiver URL.
    Cast {
        /// Stream or preview target URL for casting.
        target_url: String,
    },
    /// HDMI hardware input port number.
    HdmiSource {
        /// HDMI port number (1-based).
        port: u8,
    },
    /// Local file path or local loopback URI.
    LocalPlayer {
        /// Absolute path or URI to the local media file.
        file_path: String,
    },
    /// USB external storage fallback.
    UsbFallback {
        /// USB port number.
        port: u8,
        /// Optional mount path directory.
        mount_path: Option<String>,
    },
    /// DLNA renderer AVTransport control URL.
    DlnaRenderer {
        /// SOAP control endpoint for AVTransport service.
        control_url: String,
        /// Optional GENA event subscription endpoint.
        event_sub_url: Option<String>,
    },
}

impl MediaPath {
    /// Retrieve the path type discriminant.
    pub fn path_type(&self) -> MediaPathType {
        match self {
            Self::Cast { .. } => MediaPathType::Cast,
            Self::HdmiSource { .. } => MediaPathType::HdmiSource,
            Self::LocalPlayer { .. } => MediaPathType::LocalPlayer,
            Self::UsbFallback { .. } => MediaPathType::UsbFallback,
            Self::DlnaRenderer { .. } => MediaPathType::DlnaRenderer,
        }
    }
}

/// Criteria used to score candidate media paths.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionCriteria {
    /// Whether external network connection is available/healthy.
    pub network_available: bool,
    /// Network latency in milliseconds if measurable.
    pub network_latency_ms: Option<u32>,
    /// Whether HDMI input signal is active and locked.
    pub hdmi_signal_present: bool,
    /// Whether local file storage is accessible.
    pub local_storage_ready: bool,
    /// Whether USB storage is inserted and mounted.
    pub usb_mounted: bool,
    /// Preferred path type override from user policy if any.
    pub preferred_path: Option<MediaPathType>,
}

impl Default for SelectionCriteria {
    fn default() -> Self {
        Self {
            network_available: true,
            network_latency_ms: None,
            hdmi_signal_present: false,
            local_storage_ready: true,
            usb_mounted: false,
            preferred_path: None,
        }
    }
}

/// Media path selector that evaluates and scores available paths.
#[derive(Debug, Clone, Default)]
pub struct MediaPathSelector;

impl MediaPathSelector {
    /// Select the best media path from candidate options based on current criteria.
    pub fn select_best_path(
        candidates: &[MediaPath],
        criteria: &SelectionCriteria,
    ) -> Option<MediaPath> {
        if candidates.is_empty() {
            return None;
        }

        let mut scored: Vec<(i32, &MediaPath)> = candidates
            .iter()
            .filter_map(|path| {
                let score = Self::score_path(path, criteria)?;
                Some((score, path))
            })
            .collect();

        if scored.is_empty() {
            return None;
        }

        // Sort descending by score. Higher score wins.
        scored.sort_by_key(|a| std::cmp::Reverse(a.0));
        Some(scored[0].1.clone())
    }

    /// Calculate numerical score for a candidate path. Returns `None` if path is unusable.
    fn score_path(path: &MediaPath, criteria: &SelectionCriteria) -> Option<i32> {
        let ptype = path.path_type();

        // Check hard readiness constraints
        match path {
            MediaPath::Cast { .. } | MediaPath::DlnaRenderer { .. } => {
                if !criteria.network_available {
                    return None; // Cannot use network paths if network is offline
                }
            }
            MediaPath::HdmiSource { .. } => {
                if !criteria.hdmi_signal_present {
                    return None; // Cannot switch to HDMI if signal is not present
                }
            }
            MediaPath::LocalPlayer { .. } => {
                if !criteria.local_storage_ready {
                    return None;
                }
            }
            MediaPath::UsbFallback { .. } => {
                if !criteria.usb_mounted {
                    return None;
                }
            }
        }

        // Base score starts at 100 - (priority * 10)
        let mut score = 100 - (ptype.default_priority() as i32 * 10);

        // Preference bonus
        if let Some(preferred) = criteria.preferred_path {
            if ptype == preferred {
                score += 50;
            }
        }

        // Network latency adjustment
        if matches!(ptype, MediaPathType::Cast | MediaPathType::DlnaRenderer) {
            if let Some(latency) = criteria.network_latency_ms {
                if latency < 50 {
                    score += 15;
                } else if latency > 200 {
                    score -= 25;
                }
            }
        }

        Some(score)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_local_player_when_all_ready() {
        let candidates = vec![
            MediaPath::Cast {
                target_url: "http://cast/stream".into(),
            },
            MediaPath::LocalPlayer {
                file_path: "/media/file.mp4".into(),
            },
            MediaPath::DlnaRenderer {
                control_url: "http://10.0.0.2/ctrl".into(),
                event_sub_url: None,
            },
        ];

        let criteria = SelectionCriteria::default();
        let selected = MediaPathSelector::select_best_path(&candidates, &criteria);

        assert_eq!(selected.unwrap().path_type(), MediaPathType::LocalPlayer);
    }

    #[test]
    fn falls_back_to_usb_when_network_and_local_fail() {
        let candidates = vec![
            MediaPath::DlnaRenderer {
                control_url: "http://10.0.0.2/ctrl".into(),
                event_sub_url: None,
            },
            MediaPath::UsbFallback {
                port: 1,
                mount_path: Some("/mnt/usb".into()),
            },
        ];

        let criteria = SelectionCriteria {
            network_available: false,
            local_storage_ready: false,
            usb_mounted: true,
            ..Default::default()
        };

        let selected = MediaPathSelector::select_best_path(&candidates, &criteria);
        assert_eq!(selected.unwrap().path_type(), MediaPathType::UsbFallback);
    }

    #[test]
    fn user_preference_overrides_default_priority() {
        let candidates = vec![
            MediaPath::LocalPlayer {
                file_path: "/media/file.mp4".into(),
            },
            MediaPath::DlnaRenderer {
                control_url: "http://10.0.0.2/ctrl".into(),
                event_sub_url: None,
            },
        ];

        let criteria = SelectionCriteria {
            preferred_path: Some(MediaPathType::DlnaRenderer),
            ..Default::default()
        };

        let selected = MediaPathSelector::select_best_path(&candidates, &criteria);
        assert_eq!(selected.unwrap().path_type(), MediaPathType::DlnaRenderer);
    }
}
