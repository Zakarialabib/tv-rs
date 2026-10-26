//! Linux DRM/KMS display output probe helpers.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Rotation/orientation state reported by DRM/KMS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RotationState {
    /// Normal orientation (0 degrees).
    #[default]
    Normal,
    /// Rotated 90 degrees clockwise.
    Rotate90,
    /// Rotated 180 degrees (inverted).
    Rotate180,
    /// Rotated 270 degrees clockwise.
    Rotate270,
    /// Unknown orientation or property absent.
    Unknown,
}

impl RotationState {
    /// Parses DRM rotation property value (bitmask string or degrees).
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim().to_ascii_lowercase();
        if trimmed.contains("rotate-90") || trimmed == "90" {
            Self::Rotate90
        } else if trimmed.contains("rotate-180") || trimmed == "180" {
            Self::Rotate180
        } else if trimmed.contains("rotate-270") || trimmed == "270" {
            Self::Rotate270
        } else if trimmed.contains("rotate-0") || trimmed == "normal" || trimmed == "0" {
            Self::Normal
        } else {
            Self::Unknown
        }
    }

    /// Human-readable rotation label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Rotate90 => "rotate-90",
            Self::Rotate180 => "rotate-180",
            Self::Rotate270 => "rotate-270",
            Self::Unknown => "unknown",
        }
    }
}

/// Display mode facts probed from DRM/KMS sysfs.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DrmDisplayMode {
    /// Connector name (e.g. `HDMI-A-1`, `DP-1`).
    pub connector: String,
    /// Whether the connector is connected.
    pub connected: bool,
    /// Resolution width in pixels.
    pub width: u32,
    /// Resolution height in pixels.
    pub height: u32,
    /// Refresh rate in Hz, if available.
    pub refresh_rate: Option<u32>,
    /// Display rotation / orientation.
    pub rotation: RotationState,
}

/// Probe DRM/KMS connector facts from a given sysfs DRM connector directory path.
pub fn probe_drm_connector_dir(path: &Path) -> Option<DrmDisplayMode> {
    let dir_name = path.file_name()?.to_string_lossy().to_string();
    let (_, connector) = dir_name.split_once('-')?;

    let status = fs::read_to_string(path.join("status")).unwrap_or_default();
    let connected = status.trim() == "connected";

    let modes = fs::read_to_string(path.join("modes")).unwrap_or_default();
    let (width, height, refresh_rate) = parse_drm_modes(&modes);

    // Read orientation or rotation property if present in sysfs
    let rotation = fs::read_to_string(path.join("orientation"))
        .or_else(|_| fs::read_to_string(path.join("rotation")))
        .map(|s| RotationState::parse(&s))
        .unwrap_or(RotationState::Normal);

    Some(DrmDisplayMode {
        connector: connector.to_string(),
        connected,
        width,
        height,
        refresh_rate,
        rotation,
    })
}

/// Parses DRM modes string (e.g., `1920x1080` or `1920x1080@60`).
pub fn parse_drm_modes(modes: &str) -> (u32, u32, Option<u32>) {
    let line = modes.lines().next().unwrap_or_default().trim();
    if line.is_empty() {
        return (0, 0, None);
    }

    let (res_part, refresh_part) = if let Some((res, refresh)) = line.split_once('@') {
        (res, refresh.parse::<u32>().ok())
    } else {
        (line, None)
    };

    let Some((w_str, h_str)) = res_part.split_once('x') else {
        return (0, 0, None);
    };

    let w = w_str.trim().parse().unwrap_or(0);
    let h = h_str.trim().parse().unwrap_or(0);

    (w, h, refresh_part)
}

/// Probes all active DRM connectors under `/sys/class/drm`.
pub fn probe_drm_connectors() -> Vec<DrmDisplayMode> {
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        if let Some(mode) = probe_drm_connector_dir(&entry.path()) {
            out.push(mode);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_drm_modes() {
        assert_eq!(parse_drm_modes("1920x1080\n1280x720"), (1920, 1080, None));
        assert_eq!(
            parse_drm_modes("3840x2160@60\n1920x1080"),
            (3840, 2160, Some(60))
        );
        assert_eq!(parse_drm_modes(""), (0, 0, None));
        assert_eq!(parse_drm_modes("invalid"), (0, 0, None));
    }

    #[test]
    fn test_rotation_state_parse() {
        assert_eq!(RotationState::parse("rotate-90"), RotationState::Rotate90);
        assert_eq!(RotationState::parse("rotate-180"), RotationState::Rotate180);
        assert_eq!(RotationState::parse("rotate-270"), RotationState::Rotate270);
        assert_eq!(RotationState::parse("normal"), RotationState::Normal);
        assert_eq!(RotationState::parse("0"), RotationState::Normal);
        assert_eq!(RotationState::parse("90"), RotationState::Rotate90);
        assert_eq!(RotationState::parse("unknown_val"), RotationState::Unknown);
    }

    #[test]
    fn test_probe_drm_connector_dir() {
        let temp_dir = std::env::temp_dir().join(format!("test_drm_{}", std::process::id()));
        let connector_dir = temp_dir.join("card0-HDMI-A-1");
        fs::create_dir_all(&connector_dir).expect("create temp connector dir");

        fs::write(connector_dir.join("status"), "connected\n").unwrap();
        fs::write(connector_dir.join("modes"), "3840x2160@60\n1920x1080\n").unwrap();
        fs::write(connector_dir.join("orientation"), "rotate-90\n").unwrap();

        let mode = probe_drm_connector_dir(&connector_dir).expect("probes connector dir");
        assert_eq!(mode.connector, "HDMI-A-1");
        assert!(mode.connected);
        assert_eq!(mode.width, 3840);
        assert_eq!(mode.height, 2160);
        assert_eq!(mode.refresh_rate, Some(60));
        assert_eq!(mode.rotation, RotationState::Rotate90);

        let _ = fs::remove_dir_all(temp_dir);
    }
}
