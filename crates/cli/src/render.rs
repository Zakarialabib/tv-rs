//! Pure rendering: facade data in, printable text out.
//!
//! Every function here is a pure `&[T] -> String`. There is no I/O and no
//! async, so the exact layout — including column alignment and the empty-result
//! message — is unit-tested without a device or a network (§5, "pure core,
//! thin transport").

use tv_rs::DiscoveredTv;

/// The message printed when discovery finds nothing.
pub const NO_TELEVISONS_MESSAGE: &str = "No televisions found.";

/// The discovery table's column headers: label, vendor, IP, model, control port.
const HEADERS: [&str; 5] = ["LABEL", "VENDOR", "IP", "MODEL", "PORT"];

/// Render the discovery result as an aligned table.
///
/// Returns [`NO_TELEVISONS_MESSAGE`] when `tvs` is empty; otherwise the first
/// line is the header row and each following line is one television. Columns are
/// padded so every line is the same width, with the port column right-aligned.
pub fn render_discovery_table(tvs: &[DiscoveredTv]) -> String {
    if tvs.is_empty() {
        return NO_TELEVISONS_MESSAGE.to_string();
    }

    let rows: Vec<[String; 5]> = tvs.iter().map(discovery_row).collect();
    let widths = column_widths(&rows);

    let mut lines = Vec::with_capacity(rows.len() + 1);
    lines.push(format_row(&HEADERS.map(String::from), &widths));
    for row in &rows {
        lines.push(format_row(row, &widths));
    }

    lines.join("\n")
}

/// Build the five printable cells for one television.
fn discovery_row(tv: &DiscoveredTv) -> [String; 5] {
    let port = tv
        .vendor
        .control_port()
        .map(|port| port.to_string())
        .unwrap_or_else(|| "-".to_string());

    [
        tv.label().to_string(),
        tv.vendor.as_str().to_string(),
        tv.ip.clone(),
        tv.model_name.clone().unwrap_or_else(|| "-".to_string()),
        port,
    ]
}

/// Compute the display width of every column across the header and all rows.
fn column_widths(rows: &[[String; 5]]) -> [usize; 5] {
    let mut widths = HEADERS.map(str::len);
    for row in rows {
        for (index, cell) in row.iter().enumerate() {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }
    widths
}

/// Pad one row to the computed widths; the port column is right-aligned.
fn format_row(cells: &[String; 5], widths: &[usize; 5]) -> String {
    let label = format!("{:<width$}", cells[0], width = widths[0]);
    let vendor = format!("{:<width$}", cells[1], width = widths[1]);
    let ip = format!("{:<width$}", cells[2], width = widths[2]);
    let model = format!("{:<width$}", cells[3], width = widths[3]);
    let port = format!("{:>width$}", cells[4], width = widths[4]);
    format!("{label}  {vendor}  {ip}  {model}  {port}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tv_rs::TvVendor;

    fn tv(
        vendor: TvVendor,
        ip: &str,
        model: Option<&str>,
        friendly_name: Option<&str>,
    ) -> DiscoveredTv {
        DiscoveredTv {
            vendor,
            ip: ip.to_string(),
            location: None,
            friendly_name: friendly_name.map(str::to_string),
            manufacturer: None,
            model_name: model.map(str::to_string),
            serial_number: None,
            usn: None,
        }
    }

    #[test]
    fn empty_input_produces_the_exact_no_televisions_message() {
        assert_eq!(render_discovery_table(&[]), "No televisions found.");
        assert_eq!(render_discovery_table(&[]), NO_TELEVISONS_MESSAGE);
    }

    #[test]
    fn discovers_nothing_without_printing_a_header_only_table() {
        let rendered = render_discovery_table(&[]);

        assert!(!rendered.contains("LABEL"));
        assert_eq!(rendered.lines().count(), 1);
    }

    #[test]
    fn one_row_renders_a_header_and_an_aligned_row() {
        let tvs = [tv(
            TvVendor::Roku,
            "192.168.1.50",
            Some("Roku Ultra"),
            Some("Living Room"),
        )];
        let rendered = render_discovery_table(&tvs);
        let lines: Vec<&str> = rendered.lines().collect();

        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("LABEL"));
        assert!(lines[0].contains("PORT"));
        assert!(lines[1].contains("Living Room"));
        assert!(lines[1].contains("8060"));
        assert_eq!(lines[0].len(), lines[1].len(), "columns must align");
    }

    #[test]
    fn three_rows_render_aligned_columns() {
        let tvs = [
            tv(
                TvVendor::Roku,
                "192.168.1.50",
                Some("Roku Ultra"),
                Some("Living Room"),
            ),
            tv(TvVendor::Samsung, "192.168.1.51", Some("QE65"), None),
            tv(TvVendor::Lg, "192.168.1.52", None, None),
        ];
        let rendered = render_discovery_table(&tvs);
        let lines: Vec<&str> = rendered.lines().collect();

        assert_eq!(lines.len(), 4);
        let width = lines[0].len();
        assert!(
            lines.iter().all(|line| line.len() == width),
            "every row must be the same width:\n{rendered}"
        );
    }

    #[test]
    fn vendor_column_comes_from_tv_vendor_as_str() {
        for vendor in [TvVendor::Samsung, TvVendor::Lg, TvVendor::Roku] {
            let rendered = render_discovery_table(&[tv(vendor, "10.0.0.1", None, None)]);
            assert!(
                rendered.contains(vendor.as_str()),
                "expected '{}' in:\n{rendered}",
                vendor.as_str()
            );
        }
    }

    #[test]
    fn missing_model_and_port_fall_back_to_a_dash() {
        let rendered = render_discovery_table(&[tv(TvVendor::Unknown, "10.0.0.9", None, None)]);

        assert!(rendered.contains("unknown"));
        assert!(rendered.contains('-'));
    }
}

/// One row of the `status` table.
#[derive(Debug, Clone)]
pub struct StatusRow {
    /// Vendor label (`samsung`, `lg`, `roku`, `unknown`).
    pub vendor: String,
    /// Human label: name, model, else address.
    pub label: String,
    /// Network address.
    pub address: String,
    /// Power state, or the probe failure.
    pub power: String,
    /// Advertised capabilities, comma separated.
    pub capabilities: String,
}

/// Renders the `status` table.
///
/// Capabilities are printed because they are the honest answer to "why is that
/// button disabled" — the crate refuses what it cannot do, and this column says
/// which refusals to expect.
#[must_use]
pub fn render_status_table(rows: &[StatusRow]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{:<8} {:<24} {:<16} {:<24} {}\n",
        "VENDOR", "LABEL", "ADDRESS", "POWER", "CAPABILITIES"
    ));
    for row in rows {
        out.push_str(&format!(
            "{:<8} {:<24} {:<16} {:<24} {}\n",
            row.vendor, row.label, row.address, row.power, row.capabilities
        ));
    }
    out
}

/// Renders one reconciliation pass.
#[must_use]
pub fn render_outcomes(outcomes: &[tv_rs_agent::Outcome]) -> String {
    let mut out = String::new();
    out.push_str(&format!("{:<20} {:<14} {}\n", "DEVICE", "RESULT", "ACTION"));
    for outcome in outcomes {
        let action = outcome
            .action
            .as_ref()
            .map(|decision| format!("{decision:?}"))
            .unwrap_or_else(|| "-".to_string());
        let detail = outcome
            .error
            .as_deref()
            .map(|error| format!(" — {error}"))
            .unwrap_or_default();
        out.push_str(&format!(
            "{:<20} {:<14} {}{}\n",
            outcome.device, outcome.result, action, detail
        ));
    }
    out
}

#[cfg(test)]
mod status_tests {
    use super::*;

    #[test]
    fn the_status_table_has_a_header_and_one_line_per_device() {
        let rows = vec![
            StatusRow {
                vendor: "roku".to_string(),
                label: "Living Room".to_string(),
                address: "10.0.0.5".to_string(),
                power: "On".to_string(),
                capabilities: "power_state,key,app_launch".to_string(),
            },
            StatusRow {
                vendor: "lg".to_string(),
                label: "Lobby".to_string(),
                address: "10.0.0.6".to_string(),
                power: "probe failed: timed out".to_string(),
                capabilities: "power_state,app_launch,volume".to_string(),
            },
        ];

        let rendered = render_status_table(&rows);
        assert_eq!(rendered.lines().count(), 3, "{rendered}");
        assert!(rendered.contains("CAPABILITIES"), "{rendered}");
        assert!(rendered.contains("probe failed"), "{rendered}");
    }

    #[test]
    fn a_skipped_outcome_renders_a_dash_rather_than_an_empty_column() {
        let outcomes = vec![tv_rs_agent::Outcome {
            device: "tv-1".to_string(),
            action: None,
            result: "skipped",
            error: None,
        }];
        let rendered = render_outcomes(&outcomes);
        assert!(rendered.contains("skipped"), "{rendered}");
        let row = rendered.lines().nth(1).expect("a data row");
        assert!(
            row.trim_end().ends_with('-'),
            "a skipped device has no action, rendered as a dash: {row:?}"
        );
    }
}
