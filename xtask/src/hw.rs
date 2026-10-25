//! `cargo xtask hw-test`: probe whatever televisions are actually on this network.
//!
//! This is the honest counterpart to the hosted test suite, which never touches a
//! device. It discovers, probes power and capabilities, prints a table, and — when
//! asked — appends a row to `docs/08-hil-results.md`. When nothing answers it says so and
//! exits successfully: "no hardware here" is a fact to record, not a build failure.

use std::time::Duration;

use tv_rs::core::DeviceHandle;
use tv_rs::{discover_with, TvClient};
use tv_rs_discovery::SsdpScanner;

/// Runs the probe.
///
/// # Errors
///
/// Returns a message when the SSDP socket cannot be opened or a flag is malformed.
pub fn run(args: &[String]) -> Result<(), String> {
    let mut timeout = Duration::from_secs(3);
    let mut write_doc = false;
    let mut json = false;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--timeout" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "--timeout needs a number of seconds".to_string())?;
                let seconds: u64 = value
                    .parse()
                    .map_err(|_| format!("'{value}' is not a number of seconds"))?;
                timeout = Duration::from_secs(seconds.max(1));
                index += 1;
            }
            "--write-doc" => write_doc = true,
            "--json" => json = true,
            other => return Err(format!("unknown argument '{other}'")),
        }
        index += 1;
    }

    let scanner = SsdpScanner::new().with_timeout(timeout);
    let discovered = block_on(discover_with(&scanner)).map_err(|error| error.to_string())?;

    // SSDP on a Windows host answers with the local UPnP host service on loopback.
    // That is not a television, and reporting it would make the tool look broken in a
    // way that hides the one fact worth knowing: there is no TV here.
    let loopback_count = discovered
        .iter()
        .filter(|tv| tv.ip.starts_with("127.") || tv.ip == "::1")
        .count();
    let mut seen = std::collections::BTreeSet::new();
    let devices: Vec<_> = discovered
        .into_iter()
        .filter(|tv| !tv.ip.starts_with("127.") && tv.ip != "::1" && tv.ip != "localhost")
        .filter(|tv| seen.insert(tv.ip.clone()))
        .collect();

    if devices.is_empty() {
        println!(
            "No televisions answered SSDP within {}s. Nothing was verified.\n\
             This is a result, not a failure: it means hardware was unreachable from this host.\n\
             {} SSDP responder(s) were seen on loopback and ignored.",
            timeout.as_secs(),
            loopback_count
        );
        if write_doc {
            println!("docs/08-hil-results.md left unchanged (there is nothing to record).");
        }
        return Ok(());
    }

    let mut rows = Vec::new();
    for tv in &devices {
        let client = match TvClient::from_discovered(tv) {
            Ok(client) => client,
            Err(error) => {
                rows.push(Probe {
                    vendor: tv.vendor.as_str().to_string(),
                    label: tv.label().to_string(),
                    address: tv.ip.clone(),
                    power: format!("no client: {error}"),
                    capabilities: String::from("-"),
                });
                continue;
            }
        };

        let power = block_on(client.power_state())
            .map(|state| format!("{state:?}"))
            .unwrap_or_else(|error| format!("probe failed: {error}"));
        let capabilities = client
            .capabilities()
            .iter()
            .map(|capability| capability.as_str())
            .collect::<Vec<_>>()
            .join(",");

        rows.push(Probe {
            vendor: tv.vendor.as_str().to_string(),
            label: tv.label().to_string(),
            address: tv.ip.clone(),
            power,
            capabilities,
        });
    }

    if json {
        let body = rows
            .iter()
            .map(|row| {
                format!(
                    "{{\"vendor\":\"{}\",\"label\":\"{}\",\"address\":\"{}\",\"power\":\"{}\",\"capabilities\":\"{}\"}}",
                    escape(&row.vendor),
                    escape(&row.label),
                    escape(&row.address),
                    escape(&row.power),
                    escape(&row.capabilities)
                )
            })
            .collect::<Vec<_>>()
            .join(",\n");
        println!("[\n{body}\n]");
    } else {
        print_table(&rows);
    }

    if write_doc {
        append_rows(&rows)?;
        println!("\nAppended {} row(s) to docs/08-hil-results.md", rows.len());
    }

    Ok(())
}

struct Probe {
    vendor: String,
    label: String,
    address: String,
    power: String,
    capabilities: String,
}

fn print_table(rows: &[Probe]) {
    println!(
        "{:<8} {:<24} {:<16} {:<22} CAPABILITIES",
        "VENDOR", "LABEL", "ADDRESS", "POWER"
    );
    for row in rows {
        println!(
            "{:<8} {:<24} {:<16} {:<22} {}",
            row.vendor, row.label, row.address, row.power, row.capabilities
        );
    }
}

/// Appends rows to the hardware table.
///
/// The date is supplied by the caller's clock; every column that could not be
/// established says `not run` rather than being guessed.
fn append_rows(rows: &[Probe]) -> Result<(), String> {
    use std::io::Write;

    let path = std::path::Path::new("docs/08-hil-results.md");
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| format!("cannot open {}: {error}", path.display()))?;

    let today = today_utc();
    for row in rows {
        let power = match row.power.as_str() {
            "On" => "on",
            "Standby" => "standby",
            "Off" => "off",
            _ => "not run",
        };
        writeln!(
            file,
            "| {today} | {} | not reported | not run | not run | yes | not run | not run | not run | {power} | not run | not run | capabilities: {} |",
            row.vendor, row.capabilities
        )
        .map_err(|error| format!("cannot write: {error}"))?;
    }

    Ok(())
}

fn today_utc() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let days = seconds / 86_400;

    // Howard Hinnant's civil_from_days, same formulation as the agent's audit log.
    let z = days as i64 + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a current-thread runtime")
        .block_on(future)
}
