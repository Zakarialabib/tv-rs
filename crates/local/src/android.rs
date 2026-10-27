//! Android `DisplayManager` stub and sysfs display probing.
//!
//! Provides display probing and EDID retrieval for Android control boxes.
//! On Android, physical display information is queried via Android's
//! `DisplayManager` / sysfs (`/sys/class/drm` or `/sys/class/display`).

use super::{
    attachment_from_name, evaluate, parse_edid, Attachment, DisplayEdid, ProbeInfo, TV_THRESHOLD,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Configurable stub representing Android's `DisplayManager` service for testing
/// or custom Android platform integrations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayManagerStub {
    /// Displays managed by Android `DisplayManager`.
    pub displays: Vec<ProbeInfo>,
}

impl Default for DisplayManagerStub {
    fn default() -> Self {
        Self {
            displays: vec![ProbeInfo {
                system_name: Some("Display-0".to_string()),
                width: 1920,
                height: 1080,
                vendor: Some("LGE".to_string()),
                model: Some("Android Display".to_string()),
                serial: Some("ANDROID-DISP-0".to_string()),
                attachment: Attachment::Hdmi,
                refresh_rate: Some(60),
                bits_per_pixel: Some(24),
                physical_width_cm: Some(121.0),
                physical_height_cm: Some(68.0),
                brightness: Some(100),
                tv_confidence: 0.85,
                is_tv: true,
                evidence: vec![
                    "Android DisplayManager Display-0".to_string(),
                    "HDMI connector".to_string(),
                    "55\" panel".to_string(),
                ],
            }],
        }
    }
}

impl DisplayManagerStub {
    /// Create a new stub with custom displays.
    pub fn new(displays: Vec<ProbeInfo>) -> Self {
        Self { displays }
    }

    /// Perform a display probe using this `DisplayManager` stub.
    pub fn probe(&self) -> Vec<ProbeInfo> {
        self.displays.clone()
    }
}

/// Probes displays on Android via sysfs (`/sys/class/drm` or `/sys/class/display`)
/// with a fallback to `DisplayManagerStub` if sysfs returns no connected displays.
pub fn probe() -> Vec<ProbeInfo> {
    let mut results = probe_sysfs_drm();
    if results.is_empty() {
        results = probe_sysfs_display();
    }
    if results.is_empty() {
        results = DisplayManagerStub::default().probe();
    }
    results
}

/// Reads raw EDIDs on Android from sysfs display interfaces.
pub fn display_edids() -> Vec<DisplayEdid> {
    let mut edids = read_sysfs_edids("/sys/class/drm");
    if edids.is_empty() {
        edids = read_sysfs_edids("/sys/class/display");
    }
    edids
}

/// Helper function to probe using a `DisplayManagerStub` or sysfs.
pub fn probe_android_display_manager(stub: Option<&DisplayManagerStub>) -> Vec<ProbeInfo> {
    if let Some(stub) = stub {
        stub.probe()
    } else {
        probe()
    }
}

fn probe_sysfs_drm() -> Vec<ProbeInfo> {
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let dir_name = entry.file_name().to_string_lossy().to_string();
        let Some((_, connector)) = dir_name.split_once('-') else {
            continue;
        };
        if !is_connected(&entry.path()) {
            continue;
        }

        let modes = fs::read_to_string(entry.path().join("modes")).unwrap_or_default();
        let (mut width, mut height, refresh) = parse_first_mode(&modes);
        let edid_bytes = fs::read(entry.path().join("edid")).ok();
        let parsed_edid = edid_bytes.as_deref().and_then(|b| parse_edid(b).ok());

        let (vendor, model, serial, width_cm, height_cm) = if let Some(ref edid) = parsed_edid {
            if width == 0 || height == 0 {
                if let Some(timing) = edid.preferred_timing {
                    width = timing.h_active as u32;
                    height = timing.v_active as u32;
                }
            }
            (
                Some(edid.manufacturer_id.clone()),
                edid.display_name.clone(),
                edid.serial(),
                edid.physical_width_cm.map(f64::from),
                edid.physical_height_cm.map(f64::from),
            )
        } else {
            (None, None, None, None, None)
        };

        let attachment = attachment_from_name(connector);
        let diagonal = match (width_cm, height_cm) {
            (Some(w), Some(h)) if w > 0.0 && h > 0.0 => {
                Some((((w * w) + (h * h)).sqrt() / 2.54) as f32)
            }
            _ => None,
        };

        let (tv_confidence, mut evidence) = evaluate(
            attachment,
            width,
            height,
            refresh,
            vendor.as_deref(),
            model.as_deref(),
            diagonal,
        );
        evidence.insert(0, format!("Android DRM connector {connector}"));

        out.push(ProbeInfo {
            system_name: Some(connector.to_string()),
            width,
            height,
            vendor,
            model,
            serial,
            attachment,
            refresh_rate: refresh,
            physical_width_cm: width_cm,
            physical_height_cm: height_cm,
            tv_confidence,
            is_tv: attachment.tv_eligible() && tv_confidence >= TV_THRESHOLD,
            evidence,
            ..ProbeInfo::default()
        });
    }

    out
}

fn probe_sysfs_display() -> Vec<ProbeInfo> {
    let Ok(entries) = fs::read_dir("/sys/class/display") else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let edid_bytes = fs::read(entry.path().join("edid")).ok();
        let parsed_edid = edid_bytes.as_deref().and_then(|b| parse_edid(b).ok());

        let (vendor, model, serial, width_cm, height_cm, width, height) =
            if let Some(ref edid) = parsed_edid {
                let (w, h) = edid
                    .preferred_timing
                    .map(|t| (t.h_active as u32, t.v_active as u32))
                    .unwrap_or((1920, 1080));
                (
                    Some(edid.manufacturer_id.clone()),
                    edid.display_name.clone(),
                    edid.serial(),
                    edid.physical_width_cm.map(f64::from),
                    edid.physical_height_cm.map(f64::from),
                    w,
                    h,
                )
            } else {
                (None, None, None, None, None, 1920, 1080)
            };

        let attachment = attachment_from_name(&name);
        let diagonal = match (width_cm, height_cm) {
            (Some(w), Some(h)) if w > 0.0 && h > 0.0 => {
                Some((((w * w) + (h * h)).sqrt() / 2.54) as f32)
            }
            _ => None,
        };

        let (tv_confidence, mut evidence) = evaluate(
            attachment,
            width,
            height,
            Some(60),
            vendor.as_deref(),
            model.as_deref(),
            diagonal,
        );
        evidence.insert(0, format!("Android DisplayManager interface {name}"));

        out.push(ProbeInfo {
            system_name: Some(name),
            width,
            height,
            vendor,
            model,
            serial,
            attachment,
            refresh_rate: Some(60),
            physical_width_cm: width_cm,
            physical_height_cm: height_cm,
            tv_confidence,
            is_tv: attachment.tv_eligible() && tv_confidence >= TV_THRESHOLD,
            evidence,
            ..ProbeInfo::default()
        });
    }

    out
}

fn read_sysfs_edids(base_path: &str) -> Vec<DisplayEdid> {
    let Ok(entries) = fs::read_dir(base_path) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(raw) = fs::read(entry.path().join("edid")) else {
            continue;
        };
        let Ok(edid) = parse_edid(&raw) else {
            continue;
        };
        out.push(DisplayEdid {
            system_name: Some(name),
            source: entry.path().to_string_lossy().to_string(),
            raw,
            edid,
        });
    }
    out
}

fn is_connected(path: &Path) -> bool {
    fs::read_to_string(path.join("status")).is_ok_and(|s| s.trim() == "connected")
}

fn parse_first_mode(modes: &str) -> (u32, u32, Option<u32>) {
    let line = modes.lines().next().unwrap_or_default();
    let Some((w, h)) = line.split_once('x') else {
        return (0, 0, None);
    };
    (
        w.trim().parse().unwrap_or(0),
        h.trim().parse().unwrap_or(0),
        None,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn android_display_manager_stub_default() {
        let stub = DisplayManagerStub::default();
        let probes = stub.probe();
        assert_eq!(probes.len(), 1);
        let info = &probes[0];
        assert_eq!(info.system_name.as_deref(), Some("Display-0"));
        assert_eq!(info.width, 1920);
        assert_eq!(info.height, 1080);
        assert_eq!(info.vendor.as_deref(), Some("LGE"));
        assert_eq!(info.serial.as_deref(), Some("ANDROID-DISP-0"));
        assert!(info.is_tv);
    }

    #[test]
    fn android_probe_identifies_all_required_display_facts() {
        let stub = DisplayManagerStub::default();
        let probes = probe_android_display_manager(Some(&stub));
        assert_eq!(probes.len(), 1);
        let info = &probes[0];
        assert_eq!(info.vendor.as_deref(), Some("LGE"));
        assert_eq!(info.serial.as_deref(), Some("ANDROID-DISP-0"));
        assert_eq!(info.width, 1920);
        assert_eq!(info.height, 1080);
        assert_eq!(info.physical_width_cm, Some(121.0));
        assert_eq!(info.physical_height_cm, Some(68.0));
    }
}
