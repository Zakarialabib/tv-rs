//! Local display probing: connector, EDID identity and TV likelihood.
//!
//! This crate answers the questions that need no network and no TV control:
//! which physical connector a display uses, what EDID says about it, how large
//! it is, and how likely it is to be a television. It is deliberately
//! independent of Tauri, SQLite and any signage model.
//!
//! Platforms:
//! - Windows: WMI (`root\WMI`) for EDID-derived identity + connector, and
//!   `EnumDisplayDevicesW` / `EnumDisplaySettingsW` for the `\\.\DISPLAYn`
//!   mapping and current mode.
//! - Linux: `/sys/class/drm` connectors + EDID.
//! - macOS / other: no native probe yet; returns an empty list and the caller
//!   uses [`fallback`] with the mode Tauri/CoreGraphics already reported.
//!
//! # Example
//!
//! ```
//! use tv_rs_local::{Attachment, ProbeInfo};
//!
//! let probe = ProbeInfo {
//!     attachment: Attachment::Hdmi,
//!     tv_confidence: 0.8,
//!     is_tv: true,
//!     ..ProbeInfo::default()
//! };
//! assert!(probe.is_hdmi());
//! assert_eq!(probe.connection_type().as_deref(), Some("HDMI"));
//! ```
//!
//! The probe is synchronous and COM-bound on Windows; call it through
//! [`probe_displays_async`] from an async runtime.

#![deny(unsafe_code)] // Platform FFI is isolated in the Windows module.

use serde::{Deserialize, Serialize};

/// Physical connector class. `Unknown` is a first-class value: it means the
/// probe could not establish the connector, not "not a TV".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Attachment {
    /// Built-in laptop/panel connector (eDP, LVDS, DSI).
    Internal,
    /// HDMI.
    Hdmi,
    /// DisplayPort.
    DisplayPort,
    /// DVI.
    Dvi,
    /// VGA.
    Vga,
    /// Wireless display link (Miracast and similar).
    Wireless,
    /// Connector not established.
    #[default]
    Unknown,
}

impl Attachment {
    /// Human-readable connection label used by the UI, if known.
    pub fn connection_type(self) -> Option<&'static str> {
        match self {
            Attachment::Internal => Some("Internal"),
            Attachment::Hdmi => Some("HDMI"),
            Attachment::DisplayPort => Some("DisplayPort"),
            Attachment::Dvi => Some("DVI"),
            Attachment::Vga => Some("VGA"),
            Attachment::Wireless => Some("Wireless"),
            Attachment::Unknown => None,
        }
    }

    /// Whether this is an HDMI connector.
    pub fn is_hdmi(self) -> bool {
        matches!(self, Attachment::Hdmi)
    }

    /// Internal panels are never TVs; every other connector is eligible.
    fn tv_eligible(self) -> bool {
        !matches!(self, Attachment::Internal)
    }
}

/// Facts for one physical display.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProbeInfo {
    /// Platform system name (`\\.\DISPLAY1` on Windows, `HDMI-A-1` on Linux).
    pub system_name: Option<String>,
    /// Current mode width in pixels.
    pub width: u32,
    /// Current mode height in pixels.
    pub height: u32,
    /// Three-letter EDID manufacturer id, when available.
    pub vendor: Option<String>,
    /// EDID product name or product code, when available.
    pub model: Option<String>,
    /// EDID serial number, when available.
    pub serial: Option<String>,
    /// Physical connector class.
    pub attachment: Attachment,
    /// Current mode refresh rate in Hz.
    pub refresh_rate: Option<u32>,
    /// Current mode bits per pixel.
    pub bits_per_pixel: Option<u32>,
    /// Physical panel width in centimetres.
    pub physical_width_cm: Option<f64>,
    /// Physical panel height in centimetres.
    pub physical_height_cm: Option<f64>,
    /// Current brightness, when exposed by the platform.
    pub brightness: Option<u32>,
    /// 0.0..=1.0 confidence that this display is a television.
    pub tv_confidence: f32,
    /// Whether the probe classifies this display as a television.
    pub is_tv: bool,
    /// Short, factual lines for the UI "Evidence" list. Never secrets.
    pub evidence: Vec<String>,
}

impl ProbeInfo {
    /// Whether this display uses HDMI.
    pub fn is_hdmi(&self) -> bool {
        self.attachment.is_hdmi()
    }

    /// Human-readable connection label, if known.
    pub fn connection_type(&self) -> Option<String> {
        self.attachment.connection_type().map(str::to_string)
    }
}

/// A source of local display probes.
///
/// Implement this in tests or in a host that has its own enumeration API.
pub trait DisplayProbe {
    /// Return one entry per attached display.
    fn probe(&self) -> Vec<ProbeInfo>;
}

/// The platform's display probe.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemProbe;

impl DisplayProbe for SystemProbe {
    fn probe(&self) -> Vec<ProbeInfo> {
        probe_displays()
    }
}

/// Probe every attached display. Returns an empty list when the platform has
/// no implementation; callers must then use [`fallback`].
pub fn probe_displays() -> Vec<ProbeInfo> {
    #[cfg(target_os = "windows")]
    {
        win::probe()
    }
    #[cfg(target_os = "linux")]
    {
        linux::probe()
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        Vec::new()
    }
}

/// Async wrapper: the Windows path performs COM/WMI work that must not block
/// an async runtime.
pub async fn probe_displays_async() -> Vec<ProbeInfo> {
    tokio::task::spawn_blocking(probe_displays)
        .await
        .unwrap_or_default()
}

/// Match a probed entry to a Tauri monitor by system name, falling back to
/// index order.
pub fn probe_for<'a>(
    probes: &'a [ProbeInfo],
    system_name: Option<&str>,
    index: usize,
) -> Option<&'a ProbeInfo> {
    if let Some(name) = system_name {
        let key = connector_key(name);
        if let Some(p) = probes.iter().find(|p| {
            p.system_name
                .as_deref()
                .is_some_and(|s| connector_key(s) == key)
        }) {
            return Some(p);
        }
    }
    probes.get(index)
}

/// Best-effort probe when no platform data exists (or the name does not
/// match). The result is deliberately low-confidence so the UI says
/// "Unknown" rather than guessing a TV.
pub fn fallback(
    system_name: Option<&str>,
    width: u32,
    height: u32,
    refresh: Option<u32>,
) -> ProbeInfo {
    let attachment = system_name
        .map(attachment_from_name)
        .unwrap_or(Attachment::Unknown);
    let (tv_confidence, evidence) = evaluate(attachment, width, height, refresh, None, None, None);
    ProbeInfo {
        system_name: system_name.map(str::to_string),
        width,
        height,
        attachment,
        refresh_rate: refresh,
        tv_confidence,
        is_tv: attachment.tv_eligible() && tv_confidence >= TV_THRESHOLD,
        evidence,
        ..ProbeInfo::default()
    }
}

const TV_THRESHOLD: f32 = 0.6;

/// Connector normalization for matching Tauri monitor names to probe names.
/// `\\.\DISPLAY1` -> `DISPLAY1`; `HDMI-A-1` -> `HDMI-1`.
fn connector_key(name: &str) -> String {
    name.trim_start_matches("\\\\.\\")
        .to_ascii_uppercase()
        .replace("-A-", "-")
}

fn attachment_from_name(name: &str) -> Attachment {
    let upper = name.to_ascii_uppercase();
    if upper.contains("EDP") || upper.contains("LVDS") || upper.contains("DSI") {
        Attachment::Internal
    } else if upper.contains("HDMI") {
        Attachment::Hdmi
    } else if upper.contains("DISPLAYPORT") || upper.contains("DP-") || upper.contains("DP1") {
        Attachment::DisplayPort
    } else if upper.contains("DVI") {
        Attachment::Dvi
    } else if upper.contains("VGA") {
        Attachment::Vga
    } else if upper.contains("WIRELESS") || upper.contains("MIRACAST") {
        Attachment::Wireless
    } else {
        Attachment::Unknown
    }
}

/// Score TV likelihood from connector + physical + timing signals.
///
/// The weights are intentional: an HDMI connector alone is not a TV. A large
/// panel, broadcast timing (50 Hz), or a TV-shaped mode pushes it over the
/// line. Explicit binding always overrides this in the UI.
fn evaluate(
    attachment: Attachment,
    width: u32,
    height: u32,
    refresh: Option<u32>,
    vendor: Option<&str>,
    model: Option<&str>,
    diagonal_inches: Option<f32>,
) -> (f32, Vec<String>) {
    if attachment == Attachment::Internal {
        return (0.02, vec!["internal panel connector".to_string()]);
    }

    let mut score = 0.0f32;
    let mut evidence = Vec::new();

    match attachment {
        Attachment::Hdmi => {
            score += 0.35;
            evidence.push("HDMI connector".to_string());
        }
        Attachment::DisplayPort => {
            score += 0.20;
            evidence.push("DisplayPort connector".to_string());
        }
        Attachment::Dvi | Attachment::Vga => {
            score += 0.10;
            evidence.push("legacy connector".to_string());
        }
        Attachment::Wireless => {
            score += 0.30;
            evidence.push("wireless display link".to_string());
        }
        Attachment::Unknown => {}
        Attachment::Internal => unreachable!(),
    }

    if let Some(diag) = diagonal_inches {
        if diag >= 42.0 {
            score += 0.35;
            evidence.push(format!("{diag:.0}\" panel"));
        } else if diag >= 32.0 {
            score += 0.22;
            evidence.push(format!("{diag:.0}\" panel"));
        } else if diag >= 27.0 {
            score += 0.08;
        } else {
            score -= 0.10;
        }
    }

    if let Some(rate) = refresh {
        if rate == 50 {
            score += 0.20;
            evidence.push("50 Hz broadcast timing".to_string());
        } else if matches!(rate, 24 | 25 | 30) {
            score += 0.10;
            evidence.push(format!("{rate} Hz cinematic timing"));
        } else if rate >= 100 {
            score -= 0.10;
            evidence.push(format!("{rate} Hz high-refresh panel"));
        }
    }

    if width >= 3840 {
        score += 0.10;
        evidence.push(format!("{width}x{height} UHD mode"));
    } else if width == 1366 && height == 768 {
        score += 0.10;
        evidence.push("1366x768 panel mode".to_string());
    } else if width == 1920 && height == 1080 && diagonal_inches.is_some_and(|d| d >= 32.0) {
        score += 0.05;
    }

    if let Some(model) = model {
        if model.to_ascii_lowercase().contains("tv") {
            score += 0.20;
            evidence.push(format!("model \"{model}\""));
        }
    }

    if let Some(vendor) = vendor {
        if is_tv_brand(vendor) && diagonal_inches.is_some_and(|d| d >= 32.0) {
            score += 0.08;
            evidence.push(format!("{vendor} panel"));
        }
    }

    let confidence = score.clamp(0.0, 0.99);
    (confidence, evidence)
}

fn is_tv_brand(vendor: &str) -> bool {
    matches!(
        vendor.to_ascii_uppercase().as_str(),
        "SAM"
            | "GSM"
            | "LGE"
            | "SNY"
            | "TCL"
            | "HIS"
            | "PHL"
            | "SHP"
            | "VIZ"
            | "TOS"
            | "JVC"
            | "INS"
            | "GRU"
            | "MET"
    )
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod win {
    #![allow(unsafe_code)]

    use super::{attachment_from_name, evaluate, Attachment, ProbeInfo, TV_THRESHOLD};
    use serde::Deserialize;
    use std::collections::HashSet;
    use std::mem::{size_of, zeroed};
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayDevicesW, EnumDisplaySettingsW, DEVMODEW, DISPLAY_DEVICEW,
        DISPLAY_DEVICE_ATTACHED_TO_DESKTOP, ENUM_CURRENT_SETTINGS,
    };
    use wmi::{COMLibrary, WMIConnection};

    #[derive(Deserialize, Debug)]
    #[serde(rename = "WmiMonitorID")]
    struct WmiMonitorId {
        #[serde(rename = "InstanceName")]
        instance_name: String,
        #[serde(rename = "ManufacturerName")]
        manufacturer_name: Vec<u16>,
        #[serde(rename = "ProductCodeID")]
        product_code_id: Vec<u16>,
        #[serde(rename = "SerialNumberID")]
        serial_number_id: Vec<u16>,
        #[serde(rename = "UserFriendlyName")]
        user_friendly_name: Vec<u16>,
    }

    #[derive(Deserialize, Debug)]
    #[serde(rename = "WmiMonitorConnectionParams")]
    struct WmiMonitorConnectionParams {
        #[serde(rename = "InstanceName")]
        instance_name: String,
        #[serde(rename = "VideoOutputTechnology")]
        video_output_technology: u32,
    }

    #[derive(Deserialize, Debug)]
    #[serde(rename = "WmiMonitorBasicDisplayParams")]
    struct WmiMonitorBasicDisplayParams {
        #[serde(rename = "InstanceName")]
        instance_name: String,
        #[serde(rename = "MaxHorizontalImageSize")]
        max_horizontal_image_size: u8,
        #[serde(rename = "MaxVerticalImageSize")]
        max_vertical_image_size: u8,
    }

    #[derive(Deserialize, Debug)]
    #[serde(rename = "WmiMonitorBrightness")]
    struct WmiMonitorBrightness {
        #[serde(rename = "InstanceName")]
        instance_name: String,
        #[serde(rename = "CurrentBrightness")]
        current_brightness: u8,
    }

    #[derive(Debug, Clone, Default)]
    struct WmiRecord {
        instance_name: String,
        vendor: Option<String>,
        model: Option<String>,
        serial: Option<String>,
        vot: Option<u32>,
        width_cm: Option<f64>,
        height_cm: Option<f64>,
        brightness: Option<u32>,
    }

    #[derive(Debug, Clone)]
    struct WinDisplay {
        system_name: String,
        device_id: String,
        width: u32,
        height: u32,
        refresh: Option<u32>,
        bpp: Option<u32>,
    }

    pub(super) fn probe() -> Vec<ProbeInfo> {
        let displays = enum_displays();
        if displays.is_empty() {
            return Vec::new();
        }

        let records = query_wmi();
        let mut used = HashSet::new();

        displays
            .into_iter()
            .map(|d| {
                let record = take_record(&mut used, &records, &d.device_id);
                build_info(d, record)
            })
            .collect()
    }

    fn build_info(d: WinDisplay, record: Option<WmiRecord>) -> ProbeInfo {
        let attachment = record
            .as_ref()
            .and_then(|r| r.vot)
            .map(attachment_from_vot)
            .unwrap_or_else(|| attachment_from_name(&d.system_name));

        let diagonal = record
            .as_ref()
            .and_then(|r| match (r.width_cm, r.height_cm) {
                (Some(w), Some(h)) if w > 0.0 && h > 0.0 => {
                    Some((((w * w) + (h * h)).sqrt() / 2.54) as f32)
                }
                _ => None,
            });

        let (tv_confidence, mut evidence) = evaluate(
            attachment,
            d.width,
            d.height,
            d.refresh,
            record.as_ref().and_then(|r| r.vendor.as_deref()),
            record.as_ref().and_then(|r| r.model.as_deref()),
            diagonal,
        );

        if let Some(r) = record.as_ref() {
            if let Some(vot) = r.vot {
                evidence.insert(0, format!("WMI VideoOutputTechnology={vot}"));
            }
            if r.serial.is_some() {
                evidence.push("EDID serial present".to_string());
            }
        }

        ProbeInfo {
            system_name: Some(d.system_name),
            width: d.width,
            height: d.height,
            vendor: record.as_ref().and_then(|r| r.vendor.clone()),
            model: record.as_ref().and_then(|r| r.model.clone()),
            serial: record.as_ref().and_then(|r| r.serial.clone()),
            attachment,
            refresh_rate: d.refresh,
            bits_per_pixel: d.bpp,
            physical_width_cm: record.as_ref().and_then(|r| r.width_cm),
            physical_height_cm: record.as_ref().and_then(|r| r.height_cm),
            brightness: record.as_ref().and_then(|r| r.brightness),
            tv_confidence,
            is_tv: attachment.tv_eligible() && tv_confidence >= TV_THRESHOLD,
            evidence,
        }
    }

    fn query_wmi() -> Vec<WmiRecord> {
        let Ok(com) = COMLibrary::new() else {
            return Vec::new();
        };
        let Ok(wmi) = WMIConnection::with_namespace_path("ROOT\\WMI", com) else {
            return Vec::new();
        };

        let ids = wmi.query::<WmiMonitorId>().unwrap_or_default();
        let conn = wmi
            .query::<WmiMonitorConnectionParams>()
            .unwrap_or_default();
        let basic = wmi
            .query::<WmiMonitorBasicDisplayParams>()
            .unwrap_or_default();
        let brightness = wmi.query::<WmiMonitorBrightness>().unwrap_or_default();

        let mut records: Vec<WmiRecord> = ids
            .into_iter()
            .map(|id| WmiRecord {
                instance_name: id.instance_name,
                vendor: wide_to_string(&id.manufacturer_name),
                model: wide_to_string(&id.user_friendly_name)
                    .or_else(|| wide_to_string(&id.product_code_id)),
                serial: wide_to_string(&id.serial_number_id),
                ..WmiRecord::default()
            })
            .collect();

        for c in conn {
            if let Some(r) = records
                .iter_mut()
                .find(|r| r.instance_name == c.instance_name)
            {
                r.vot = Some(c.video_output_technology);
            }
        }
        for b in basic {
            if let Some(r) = records
                .iter_mut()
                .find(|r| r.instance_name == b.instance_name)
            {
                r.width_cm = Some(b.max_horizontal_image_size as f64);
                r.height_cm = Some(b.max_vertical_image_size as f64);
            }
        }
        for b in brightness {
            if let Some(r) = records
                .iter_mut()
                .find(|r| r.instance_name == b.instance_name)
            {
                r.brightness = Some(b.current_brightness as u32);
            }
        }

        records
    }

    fn take_record(
        used: &mut HashSet<usize>,
        records: &[WmiRecord],
        device_id: &str,
    ) -> Option<WmiRecord> {
        if let Some(hw) = hw_id(device_id) {
            if let Some((i, record)) = records.iter().enumerate().find(|(i, r)| {
                !used.contains(i) && hw_id(&r.instance_name).as_deref() == Some(hw.as_str())
            }) {
                used.insert(i);
                return Some(record.clone());
            }
            // A hardware id that does not match must not be assigned by
            // position: that would put the wrong EDID on the wrong screen.
            return None;
        }

        let next = (0..records.len()).find(|i| !used.contains(i))?;
        used.insert(next);
        records.get(next).cloned()
    }

    fn hw_id(raw: &str) -> Option<String> {
        let mut parts = raw.split('\\');
        let prefix = parts.next()?;
        if !prefix.eq_ignore_ascii_case("DISPLAY") && !prefix.eq_ignore_ascii_case("MONITOR") {
            return None;
        }
        let id = parts.next()?.trim().to_ascii_uppercase();
        if id.is_empty() {
            None
        } else {
            Some(id)
        }
    }

    fn attachment_from_vot(vot: u32) -> Attachment {
        match vot {
            0 => Attachment::Vga, // D3DKMDT_VOT_HD15
            4 => Attachment::Dvi,
            5 => Attachment::Hdmi,
            6 | 11 => Attachment::Internal, // LVDS, DP embedded
            10 => Attachment::DisplayPort,
            15 => Attachment::Wireless, // Miracast
            0x8000_0000 => Attachment::Internal,
            _ => Attachment::Unknown,
        }
    }

    fn wide_to_string(raw: &[u16]) -> Option<String> {
        let text: String = raw
            .iter()
            .take_while(|&&c| c != 0)
            .map(|&c| char::from_u32(c as u32).unwrap_or('\u{FFFD}'))
            .collect();
        let text = text.trim().to_string();
        if text.is_empty() {
            None
        } else {
            Some(text)
        }
    }

    /// Two-level `EnumDisplayDevicesW`: adapters (`\\.\DISPLAYn`) then their
    /// monitor entries (which carry the PNP hardware id).
    fn enum_displays() -> Vec<WinDisplay> {
        let mut out = Vec::new();

        for adapter_index in 0..32 {
            let mut adapter: DISPLAY_DEVICEW = unsafe { zeroed() };
            adapter.cb = size_of::<DISPLAY_DEVICEW>() as u32;
            let ok =
                unsafe { EnumDisplayDevicesW(std::ptr::null(), adapter_index, &mut adapter, 0) };
            if ok == 0 {
                break;
            }
            if adapter.StateFlags & DISPLAY_DEVICE_ATTACHED_TO_DESKTOP == 0 {
                continue;
            }

            let system_name = wide_to_string(&adapter.DeviceName).unwrap_or_default();
            let (width, height, refresh, bpp) = mode_for(&adapter.DeviceName);

            let mut monitor_index = 0u32;
            let mut found = false;
            loop {
                let mut monitor: DISPLAY_DEVICEW = unsafe { zeroed() };
                monitor.cb = size_of::<DISPLAY_DEVICEW>() as u32;
                let ok = unsafe {
                    EnumDisplayDevicesW(adapter.DeviceName.as_ptr(), monitor_index, &mut monitor, 0)
                };
                if ok == 0 {
                    break;
                }
                let device_id = wide_to_string(&monitor.DeviceID).unwrap_or_default();
                out.push(WinDisplay {
                    system_name: system_name.clone(),
                    device_id,
                    width,
                    height,
                    refresh,
                    bpp,
                });
                found = true;
                monitor_index += 1;
            }

            if !found {
                out.push(WinDisplay {
                    system_name,
                    device_id: wide_to_string(&adapter.DeviceID).unwrap_or_default(),
                    width,
                    height,
                    refresh,
                    bpp,
                });
            }
        }

        out
    }

    fn mode_for(device_name: &[u16]) -> (u32, u32, Option<u32>, Option<u32>) {
        let mut mode: DEVMODEW = unsafe { zeroed() };
        mode.dmSize = size_of::<DEVMODEW>() as u16;
        let ok =
            unsafe { EnumDisplaySettingsW(device_name.as_ptr(), ENUM_CURRENT_SETTINGS, &mut mode) };
        if ok == 0 {
            return (0, 0, None, None);
        }
        let refresh = if mode.dmDisplayFrequency > 1 {
            Some(mode.dmDisplayFrequency)
        } else {
            None
        };
        let bpp = if mode.dmBitsPerPel > 0 {
            Some(mode.dmBitsPerPel)
        } else {
            None
        };
        (mode.dmPelsWidth, mode.dmPelsHeight, refresh, bpp)
    }
}

// ---------------------------------------------------------------------------
// Linux
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod linux {
    use super::{attachment_from_name, evaluate, ProbeInfo, TV_THRESHOLD};
    use std::fs;

    pub(super) fn probe() -> Vec<ProbeInfo> {
        let Ok(entries) = fs::read_dir("/sys/class/drm") else {
            return Vec::new();
        };

        let mut out = Vec::new();
        for entry in entries.flatten() {
            let dir_name = entry.file_name().to_string_lossy().to_string();
            let Some((_, connector)) = dir_name.split_once('-') else {
                continue;
            };
            if fs::read_to_string(entry.path().join("status"))
                .map(|s| s.trim() != "connected")
                .unwrap_or(true)
            {
                continue;
            }

            let modes = fs::read_to_string(entry.path().join("modes")).unwrap_or_default();
            let (width, height, refresh) = parse_first_mode(&modes);
            let edid = fs::read(entry.path().join("edid")).ok();
            let (vendor, model, width_cm, height_cm) = edid
                .as_deref()
                .map(parse_edid)
                .unwrap_or((None, None, None, None));

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
            evidence.insert(0, format!("DRM connector {connector}"));

            out.push(ProbeInfo {
                system_name: Some(connector.to_string()),
                width,
                height,
                vendor,
                model,
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

    /// `modes` is a newline-separated list like `1920x1080`; refresh is not
    /// exposed there, so it stays `None`.
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

    fn parse_edid(edid: &[u8]) -> (Option<String>, Option<String>, Option<f64>, Option<f64>) {
        if edid.len() < 128 || &edid[0..8] != [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00] {
            return (None, None, None, None);
        }

        // Manufacturer id: three 5-bit letters, big-endian at bytes 8-9.
        let raw = ((edid[8] as u16) << 8) | edid[9] as u16;
        let mut vendor = String::new();
        for shift in [10u16, 5, 0] {
            let code = ((raw >> shift) & 0x1F) as u8;
            if (1..=26).contains(&code) {
                vendor.push((b'A' + code - 1) as char);
            }
        }
        let vendor = if vendor.is_empty() || vendor == "AAA" {
            None
        } else {
            Some(vendor)
        };

        let width_cm = (edid[21] > 0).then_some(edid[21] as f64);
        let height_cm = (edid[22] > 0).then_some(edid[22] as f64);

        (vendor, None, width_cm, height_cm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hdmi_large_panel_is_tv() {
        let (confidence, evidence) = evaluate(
            Attachment::Hdmi,
            3840,
            2160,
            Some(60),
            Some("SAM"),
            None,
            Some(55.0),
        );
        assert!(confidence >= TV_THRESHOLD, "confidence {confidence}");
        assert!(evidence.iter().any(|e| e.contains("HDMI")));
        assert!(evidence.iter().any(|e| e.contains("55")));
    }

    #[test]
    fn small_hdmi_monitor_is_not_tv() {
        let (confidence, _) = evaluate(
            Attachment::Hdmi,
            1920,
            1080,
            Some(60),
            Some("SAM"),
            None,
            Some(27.0),
        );
        assert!(confidence < TV_THRESHOLD, "confidence {confidence}");
    }

    #[test]
    fn internal_panel_is_never_tv() {
        let (confidence, evidence) = evaluate(
            Attachment::Internal,
            1920,
            1080,
            Some(60),
            None,
            None,
            Some(16.0),
        );
        assert!(confidence < 0.1);
        assert_eq!(evidence, vec!["internal panel connector"]);
    }

    #[test]
    fn broadcast_timing_helps() {
        let (without, _) = evaluate(
            Attachment::Hdmi,
            1920,
            1080,
            Some(60),
            None,
            None,
            Some(32.0),
        );
        let (with, _) = evaluate(
            Attachment::Hdmi,
            1920,
            1080,
            Some(50),
            None,
            None,
            Some(32.0),
        );
        assert!(with > without);
    }

    #[test]
    fn name_heuristics() {
        assert_eq!(attachment_from_name(r"\\.\DISPLAY1"), Attachment::Unknown);
        assert_eq!(attachment_from_name("HDMI-A-1"), Attachment::Hdmi);
        assert_eq!(attachment_from_name("eDP-1"), Attachment::Internal);
        assert_eq!(attachment_from_name("DP-2"), Attachment::DisplayPort);
    }

    #[test]
    fn connector_keys_match_windows_and_linux_shapes() {
        assert_eq!(connector_key(r"\\.\DISPLAY1"), "DISPLAY1");
        assert_eq!(connector_key("HDMI-A-1"), "HDMI-1");
        assert_eq!(connector_key("HDMI-1"), "HDMI-1");
    }

    #[test]
    fn fallback_is_conservative() {
        let info = fallback(Some("HDMI-1"), 1920, 1080, Some(60));
        assert_eq!(info.attachment, Attachment::Hdmi);
        assert!(!info.is_tv, "no physical size means no TV claim");
    }

    #[test]
    fn fallback_keeps_mode_and_probe_for_matches_connector() {
        let info = fallback(Some("HDMI-A-1"), 3840, 2160, Some(50));
        assert_eq!(info.width, 3840);
        assert_eq!(info.height, 2160);
        assert_eq!(info.refresh_rate, Some(50));

        let probes = vec![info];
        let found = probe_for(&probes, Some("HDMI-1"), 7).expect("connector match");
        assert_eq!(found, &probes[0]);
    }
}
