//! A platform-independent EDID base-block parser.
//!
//! Every field is read from the 128-byte base block as laid out in VESA
//! *Enhanced Extended Display Identification Data Standard* (E-EDID) Release A,
//! Revision 2 (EDID 1.4), section 3:
//!
//! | Bytes     | Field                                    | Spec   |
//! | --------- | ---------------------------------------- | ------ |
//! | `0..8`    | header `00 FF FF FF FF FF FF 00`         | §3.3   |
//! | `8..10`   | manufacturer id, three 5-bit letters, BE | §3.4.1 |
//! | `10..12`  | product code, little-endian              | §3.4.2 |
//! | `12..16`  | serial number, little-endian             | §3.4.3 |
//! | `16`,`17` | week and year (`+1990`) of manufacture   | §3.4.4 |
//! | `18`,`19` | EDID version and revision                | §3.5   |
//! | `20`      | video input definition (bit 7: digital)  | §3.6.1 |
//! | `21`,`22` | horizontal / vertical screen size in cm  | §3.6.2 |
//! | `54..126` | four 18-byte descriptors                 | §3.10  |
//! | `126`     | extension block count                    | §3.11  |
//! | `127`     | checksum: all 128 bytes sum to 0 mod 256 | §3.11  |
//!
//! Extension blocks (CTA-861, DisplayID) are counted, not decoded.
//!
//! ```
//! use tv_rs_local::edid::{parse_edid, EdidError};
//!
//! assert!(matches!(parse_edid(&[0u8; 16]), Err(EdidError::TooShort(16))));
//! ```

use super::{evaluate, Attachment, ProbeInfo, TV_THRESHOLD};
use serde::{Deserialize, Serialize};

/// The fixed eight-byte EDID header (E-EDID §3.3).
pub const EDID_HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];

/// Length of the EDID base block and of every extension block.
pub const EDID_BLOCK_LEN: usize = 128;

/// Why a byte slice is not an EDID base block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdidError {
    /// Fewer than 128 bytes were supplied.
    TooShort(usize),
    /// The first eight bytes are not the EDID header.
    BadHeader,
}

impl std::fmt::Display for EdidError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort(len) => write!(f, "EDID is {len} bytes; the base block is 128"),
            Self::BadHeader => f.write_str("EDID header is not 00 FF FF FF FF FF FF 00"),
        }
    }
}

impl std::error::Error for EdidError {}

/// A detailed timing descriptor (E-EDID §3.10.2).
///
/// The first one in the base block is the preferred mode when the feature
/// support byte says so (always, from EDID 1.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DetailedTiming {
    /// Pixel clock in kHz (the wire value is in units of 10 kHz).
    pub pixel_clock_khz: u32,
    /// Horizontal addressable pixels.
    pub h_active: u16,
    /// Horizontal blanking pixels.
    pub h_blank: u16,
    /// Vertical addressable lines (per field when interlaced).
    pub v_active: u16,
    /// Vertical blanking lines.
    pub v_blank: u16,
    /// Addressable image width in millimetres, `0` when undefined.
    pub image_width_mm: u16,
    /// Addressable image height in millimetres, `0` when undefined.
    pub image_height_mm: u16,
    /// Whether the mode is interlaced.
    pub interlaced: bool,
}

impl DetailedTiming {
    /// Vertical refresh in Hz, from the pixel clock and the total raster.
    ///
    /// Returns `None` for a zero raster. For an interlaced mode this is the
    /// field rate.
    #[must_use]
    pub fn refresh_hz(&self) -> Option<f64> {
        let h_total = u32::from(self.h_active) + u32::from(self.h_blank);
        let v_total = u32::from(self.v_active) + u32::from(self.v_blank);
        let raster = f64::from(h_total) * f64::from(v_total);
        (raster > 0.0).then(|| f64::from(self.pixel_clock_khz) * 1000.0 / raster)
    }

    fn parse(d: &[u8]) -> Option<Self> {
        let clock = u16::from_le_bytes([d[0], d[1]]);
        if clock == 0 {
            return None;
        }
        let hi = |byte: u8| u16::from(byte >> 4) << 8;
        let lo = |byte: u8| u16::from(byte & 0x0F) << 8;
        Some(Self {
            pixel_clock_khz: u32::from(clock) * 10,
            h_active: u16::from(d[2]) | hi(d[4]),
            h_blank: u16::from(d[3]) | lo(d[4]),
            v_active: u16::from(d[5]) | hi(d[7]),
            v_blank: u16::from(d[6]) | lo(d[7]),
            image_width_mm: u16::from(d[12]) | hi(d[14]),
            image_height_mm: u16::from(d[13]) | lo(d[14]),
            interlaced: d[17] & 0x80 != 0,
        })
    }
}

/// What the EDID base block says about a display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Edid {
    /// Three-letter PNP manufacturer id (`SAM`, `GSM`, `DEL`).
    pub manufacturer_id: String,
    /// Manufacturer product code.
    pub product_code: u16,
    /// Numeric serial number; `None` when the field is zero (unused).
    pub serial_number: Option<u32>,
    /// Week of manufacture, 1–54; `None` when unspecified or a model year.
    pub manufacture_week: Option<u8>,
    /// Year of manufacture (or model year when the week byte is `0xFF`).
    pub manufacture_year: u16,
    /// EDID structure version (`1`).
    pub version: u8,
    /// EDID structure revision (`3`, `4`).
    pub revision: u8,
    /// Whether the video input is digital.
    pub digital: bool,
    /// Screen width in centimetres; `None` when undefined or only an aspect
    /// ratio is given (one of the two size bytes is zero, §3.6.2).
    pub physical_width_cm: Option<u8>,
    /// Screen height in centimetres; `None` under the same rule as the width.
    pub physical_height_cm: Option<u8>,
    /// Display product name descriptor (tag `0xFC`).
    pub display_name: Option<String>,
    /// Display product serial number descriptor (tag `0xFF`).
    pub serial_string: Option<String>,
    /// The first detailed timing descriptor: the preferred mode.
    pub preferred_timing: Option<DetailedTiming>,
    /// Number of extension blocks that follow the base block.
    pub extension_count: u8,
    /// Whether the base block's checksum is correct. A bad checksum is
    /// reported, not rejected: some panels ship one, and the identity fields
    /// are still the best facts available.
    pub checksum_valid: bool,
}

impl Edid {
    /// Screen diagonal in inches from the centimetre size fields.
    #[must_use]
    pub fn diagonal_inches(&self) -> Option<f64> {
        let w = f64::from(self.physical_width_cm?);
        let h = f64::from(self.physical_height_cm?);
        Some((w * w + h * h).sqrt() / 2.54)
    }

    /// The best model label: the product name descriptor, else the product
    /// code as four hex digits (the form Windows uses in `SAM0F99`).
    #[must_use]
    pub fn model(&self) -> String {
        self.display_name
            .clone()
            .unwrap_or_else(|| format!("{:04X}", self.product_code))
    }

    /// The best serial label: the serial descriptor, else the numeric serial.
    #[must_use]
    pub fn serial(&self) -> Option<String> {
        self.serial_string
            .clone()
            .or_else(|| self.serial_number.map(|n| n.to_string()))
    }

    /// The Windows/PNP hardware id: manufacturer plus product code in hex,
    /// e.g. `SAM0F99`. This is the key under `Enum\DISPLAY` in the registry.
    #[must_use]
    pub fn pnp_id(&self) -> String {
        format!("{}{:04X}", self.manufacturer_id, self.product_code)
    }

    /// Returns the native panel resolution in pixels `(width, height)` if preferred timing is available.
    #[must_use]
    pub fn preferred_resolution(&self) -> Option<(u32, u32)> {
        self.preferred_timing
            .map(|t| (u32::from(t.h_active), u32::from(t.v_active)))
    }

    /// Converts this parsed EDID into a [`ProbeInfo`] structure.
    #[must_use]
    pub fn to_probe_info(&self, system_name: Option<String>, attachment: Attachment) -> ProbeInfo {
        let (width, height) = self.preferred_resolution().unwrap_or((0, 0));
        let refresh_rate = self
            .preferred_timing
            .and_then(|t| t.refresh_hz().map(|r| r.round() as u32));
        let physical_width_cm = self.physical_width_cm.map(f64::from);
        let physical_height_cm = self.physical_height_cm.map(f64::from);
        let diagonal = self.diagonal_inches().map(|d| d as f32);
        let vendor = Some(self.manufacturer_id.clone());
        let model = Some(self.model());
        let serial = self.serial();

        let (tv_confidence, mut evidence) = evaluate(
            attachment,
            width,
            height,
            refresh_rate,
            vendor.as_deref(),
            model.as_deref(),
            diagonal,
        );

        if serial.is_some() {
            evidence.push("EDID serial present".to_string());
        }

        ProbeInfo {
            system_name,
            width,
            height,
            vendor,
            model,
            serial,
            attachment,
            refresh_rate,
            bits_per_pixel: None,
            physical_width_cm,
            physical_height_cm,
            brightness: None,
            tv_confidence,
            is_tv: attachment.tv_eligible() && tv_confidence >= TV_THRESHOLD,
            evidence,
        }
    }
}

/// Parses an EDID base block. Trailing extension blocks are ignored.
///
/// # Errors
///
/// [`EdidError::TooShort`] for fewer than 128 bytes and
/// [`EdidError::BadHeader`] when the fixed header is missing. A bad checksum
/// is not an error; see [`Edid::checksum_valid`].
// `chunks_exact_to_as_chunks` is a lint newer toolchains have and older ones do
// not, and an unknown lint under `-D warnings` is itself an error. Naming
// `unknown_lints` alongside it keeps the workspace clippy-clean on both.
#[allow(unknown_lints, clippy::chunks_exact_to_as_chunks)]
pub fn parse_edid(bytes: &[u8]) -> Result<Edid, EdidError> {
    if bytes.len() < EDID_BLOCK_LEN {
        return Err(EdidError::TooShort(bytes.len()));
    }
    let b = &bytes[..EDID_BLOCK_LEN];
    if b[..8] != EDID_HEADER {
        return Err(EdidError::BadHeader);
    }

    let checksum_valid = b.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)) == 0;

    let raw_id = u16::from_be_bytes([b[8], b[9]]);
    let manufacturer_id: String = [10u16, 5, 0]
        .iter()
        .map(|shift| {
            let code = ((raw_id >> shift) & 0x1F) as u8;
            if (1..=26).contains(&code) {
                char::from(b'A' + code - 1)
            } else {
                '?'
            }
        })
        .collect();

    let serial = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);
    let week = b[16];

    let (width, height) = (b[21], b[22]);
    let sized = width != 0 && height != 0;

    let mut display_name = None;
    let mut serial_string = None;
    let mut preferred_timing = None;
    for (index, descriptor) in b[54..126].chunks_exact(18).enumerate() {
        if descriptor[0] != 0 || descriptor[1] != 0 {
            if index == 0 {
                preferred_timing = DetailedTiming::parse(descriptor);
            }
            continue;
        }
        match descriptor[3] {
            0xFC if display_name.is_none() => display_name = descriptor_text(descriptor),
            0xFF if serial_string.is_none() => serial_string = descriptor_text(descriptor),
            _ => {}
        }
    }

    Ok(Edid {
        manufacturer_id,
        product_code: u16::from_le_bytes([b[10], b[11]]),
        serial_number: (serial != 0).then_some(serial),
        manufacture_week: (1..=54).contains(&week).then_some(week),
        manufacture_year: 1990 + u16::from(b[17]),
        version: b[18],
        revision: b[19],
        digital: b[20] & 0x80 != 0,
        physical_width_cm: sized.then_some(width),
        physical_height_cm: sized.then_some(height),
        display_name,
        serial_string,
        preferred_timing,
        extension_count: b[126],
        checksum_valid,
    })
}

/// Text from a display descriptor: 13 bytes, ended by `0x0A`, padded with
/// spaces (§3.10.3). Non-printable bytes are dropped.
fn descriptor_text(descriptor: &[u8]) -> Option<String> {
    let text: String = descriptor[5..18]
        .iter()
        .take_while(|&&byte| byte != 0x0A && byte != 0x00)
        .filter(|byte| byte.is_ascii_graphic() || **byte == b' ')
        .map(|&byte| char::from(byte))
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parses an `edid-decode (hex)` dump into bytes.
    fn hex(dump: &str) -> Vec<u8> {
        dump.split_whitespace()
            .map(|pair| u8::from_str_radix(pair, 16).expect("hex byte"))
            .collect()
    }

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        hex(&std::fs::read_to_string(path).expect("fixture"))
    }

    #[test]
    fn lg_tv_fixture_matches_edid_decode() {
        let bytes = fixture("lg-tv-gsm0001.hex");
        assert_eq!(bytes.len(), 256);
        let edid = parse_edid(&bytes).expect("parses");
        assert!(edid.checksum_valid);
        assert_eq!(edid.manufacturer_id, "GSM");
        assert_eq!(edid.product_code, 1);
        assert_eq!(edid.pnp_id(), "GSM0001");
        assert_eq!(edid.serial_number, Some(0x0101_0101));
        assert_eq!(edid.manufacture_week, Some(1));
        assert_eq!(edid.manufacture_year, 2015);
        assert_eq!((edid.version, edid.revision), (1, 3));
        assert!(edid.digital);
        assert_eq!(edid.physical_width_cm, Some(160));
        assert_eq!(edid.physical_height_cm, Some(90));
        assert_eq!(edid.display_name.as_deref(), Some("LG TV"));
        assert_eq!(edid.serial_string, None);
        assert_eq!(edid.serial().as_deref(), Some("16843009"));
        assert_eq!(edid.extension_count, 1);
        let diagonal = edid.diagonal_inches().expect("sized");
        assert!((diagonal - 72.27).abs() < 0.01, "{diagonal}");

        let timing = edid.preferred_timing.expect("DTD 1");
        assert_eq!(timing.pixel_clock_khz, 148_500);
        assert_eq!((timing.h_active, timing.v_active), (1920, 1080));
        assert_eq!((timing.image_width_mm, timing.image_height_mm), (1600, 900));
        assert!(!timing.interlaced);
        let hz = timing.refresh_hz().expect("raster");
        assert!((hz - 60.0).abs() < 0.001, "{hz}");
    }

    #[test]
    fn samsung_fixture_matches_edid_decode() {
        let bytes = fixture("samsung-c49rg9x-sam0f99.hex");
        let edid = parse_edid(&bytes).expect("parses");
        assert!(edid.checksum_valid);
        assert_eq!(edid.manufacturer_id, "SAM");
        assert_eq!(edid.product_code, 3993);
        assert_eq!(edid.pnp_id(), "SAM0F99");
        assert_eq!(edid.manufacture_week, Some(50));
        assert_eq!(edid.manufacture_year, 2020);
        assert_eq!(edid.physical_width_cm, Some(119));
        assert_eq!(edid.physical_height_cm, Some(34));
        assert_eq!(edid.display_name.as_deref(), Some("C49RG9x"));
        assert_eq!(edid.model(), "C49RG9x");
        assert_eq!(edid.serial_string.as_deref(), Some("HNKNC00549"));
        assert_eq!(edid.extension_count, 3);

        let timing = edid.preferred_timing.expect("DTD 1");
        assert_eq!(timing.pixel_clock_khz, 266_500);
        assert_eq!((timing.h_active, timing.v_active), (3840, 1080));
        assert_eq!((timing.image_width_mm, timing.image_height_mm), (1193, 336));
        let hz = timing.refresh_hz().expect("raster");
        assert!((hz - 59.968_497).abs() < 0.000_01, "{hz}");
    }

    #[test]
    fn a_corrupted_checksum_is_reported_not_rejected() {
        let mut bytes = fixture("lg-tv-gsm0001.hex");
        bytes[127] ^= 0xFF;
        let edid = parse_edid(&bytes).expect("still parses");
        assert!(!edid.checksum_valid);
        assert_eq!(edid.manufacturer_id, "GSM");
    }

    #[test]
    fn short_and_headerless_input_is_rejected() {
        assert_eq!(parse_edid(&[]), Err(EdidError::TooShort(0)));
        assert_eq!(parse_edid(&[0u8; 128]), Err(EdidError::BadHeader));
    }

    #[test]
    fn an_aspect_ratio_only_size_is_not_a_size() {
        let mut bytes = fixture("lg-tv-gsm0001.hex");
        bytes[22] = 0;
        let edid = parse_edid(&bytes).expect("parses");
        assert_eq!(edid.physical_width_cm, None);
        assert_eq!(edid.physical_height_cm, None);
        assert_eq!(edid.diagonal_inches(), None);
    }

    #[test]
    fn model_falls_back_to_the_product_code() {
        let mut bytes = fixture("lg-tv-gsm0001.hex");
        // Turn the 0xFC name descriptor at offset 108 into a dummy (0x10).
        bytes[108 + 3] = 0x10;
        let edid = parse_edid(&bytes).expect("parses");
        assert_eq!(edid.display_name, None);
        assert_eq!(edid.model(), "0001");
    }

    #[test]
    fn edid_to_probe_info_conversion() {
        let bytes = fixture("samsung-c49rg9x-sam0f99.hex");
        let edid = parse_edid(&bytes).expect("parses");
        let info = edid.to_probe_info(Some("HDMI-1".to_string()), Attachment::Hdmi);
        assert_eq!(info.system_name.as_deref(), Some("HDMI-1"));
        assert_eq!(info.vendor.as_deref(), Some("SAM"));
        assert_eq!(info.model.as_deref(), Some("C49RG9x"));
        assert_eq!(info.serial.as_deref(), Some("HNKNC00549"));
        assert_eq!(info.width, 3840);
        assert_eq!(info.height, 1080);
        assert_eq!(info.physical_width_cm, Some(119.0));
        assert_eq!(info.physical_height_cm, Some(34.0));
    }

    #[test]
    fn dell_vendor_edid_parsing_and_identification() {
        let mut b = vec![0u8; 128];
        b[..8].copy_from_slice(&EDID_HEADER);
        // Manufacturer ID 'DEL': D=4, E=5, L=12 => (4<<10) | (5<<5) | 12 = 4268 = 0x10AC
        b[8..10].copy_from_slice(&0x10ACu16.to_be_bytes());
        b[10..12].copy_from_slice(&0x413Cu16.to_le_bytes()); // Product code 0x413C
        b[12..16].copy_from_slice(&12345678u32.to_le_bytes()); // Serial
        b[16] = 20; // Week
        b[17] = 30; // Year 2020
        b[18] = 1;
        b[19] = 4; // EDID 1.4
        b[20] = 0x80; // Digital
        b[21] = 60; // 60 cm width
        b[22] = 34; // 34 cm height

        // Preferred timing DTD in b[54..72] for 2560x1440
        b[54..56].copy_from_slice(&2415u16.to_le_bytes());
        b[56] = 0x00; // H active low 8
        b[57] = 0xE0; // H blank low 8
        b[58] = 0xA1; // H active hi 4 (0x0A) | H blank hi 4 (0x01)
        b[59] = 0xA0; // V active low 8
        b[60] = 0x26; // V blank low 8
        b[61] = 0x50; // V active hi 4 (0x05) | V blank hi 4 (0x00)

        let sum = b[..127]
            .iter()
            .fold(0u8, |acc, byte| acc.wrapping_add(*byte));
        b[127] = (0u8).wrapping_sub(sum);

        let edid = parse_edid(&b).expect("valid dell edid");
        assert!(edid.checksum_valid);
        assert_eq!(edid.manufacturer_id, "DEL");
        assert_eq!(edid.serial(), Some("12345678".to_string()));
        assert_eq!(edid.physical_width_cm, Some(60));
        assert_eq!(edid.physical_height_cm, Some(34));

        let info = edid.to_probe_info(Some("DP-1".to_string()), Attachment::DisplayPort);
        assert_eq!(info.vendor.as_deref(), Some("DEL"));
        assert_eq!(info.serial.as_deref(), Some("12345678"));
        assert_eq!(info.width, 2560);
        assert_eq!(info.height, 1440);
        assert_eq!(info.physical_width_cm, Some(60.0));
        assert_eq!(info.physical_height_cm, Some(34.0));
    }
}
