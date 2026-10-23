//! Command execution against the [`tv_rs`] facade.
//!
//! This is the thin transport layer of the CLI: it parses arguments that are
//! already typed (thanks to `clap`), calls the facade or a protocol client, and
//! prints. All formatting lives in [`crate::render`]; all string-to-key
//! translation lives in [`crate::cli`].

use std::net::SocketAddr;
use std::time::Duration;

use tv_rs::discovery::SsdpScanner;
use tv_rs::map;
use tv_rs::power::{MacAddress, WolSender};
use tv_rs::roku::RokuClient;
use tv_rs::tizen::{TizenClient, TizenSession};
use tv_rs::{DiscoveredTv, TvClient, TvVendor};
use tv_rs_core::Capability;

use crate::cli::{
    parse_roku_key, parse_tizen_key, roku_key_names, tizen_key_names, Cli, Command, DiscoverArgs,
    EnsureArgs, KeyArgs, LaunchArgs, PowerOffArgs, StatusArgs, Vendor, WakeArgs,
};
use crate::error::CliError;
use crate::render;

/// Dispatch a parsed command line to its handler.
pub async fn execute(cli: Cli) -> Result<(), CliError> {
    match cli.command {
        Command::Discover(args) => run_discover(args).await,
        Command::Wake(args) => run_wake(args).await,
        Command::PowerOff(args) => run_power_off(args).await,
        Command::Key(args) => run_key(args).await,
        Command::Launch(args) => run_launch(args).await,
        Command::Status(args) => run_status(args).await,
        Command::Ensure(args) => run_ensure(args).await,
    }
}

/// Assemble a minimal [`DiscoveredTv`] from an address and vendor.
///
/// The control commands take an IP and vendor directly rather than running
/// discovery first, so every descriptor field is `None`.
fn discovered(vendor: TvVendor, ip: &str) -> DiscoveredTv {
    DiscoveredTv {
        vendor,
        ip: ip.to_string(),
        location: None,
        friendly_name: None,
        manufacturer: None,
        model_name: None,
        serial_number: None,
        usn: None,
    }
}

/// `tv-rs discover [--timeout <SECONDS>]`
async fn run_discover(args: DiscoverArgs) -> Result<(), CliError> {
    let found = match args.timeout {
        Some(seconds) => {
            let scanner = SsdpScanner::new().with_timeout(Duration::from_secs(seconds));
            tv_rs::discover_with(&scanner).await?
        }
        None => tv_rs::discover().await?,
    };

    println!("{}", render::render_discovery_table(&found));
    Ok(())
}

/// `tv-rs wake <MAC> [--broadcast <IP:PORT>]`
async fn run_wake(args: WakeArgs) -> Result<(), CliError> {
    let mac: MacAddress = args.mac.parse().map_err(|source| CliError::MacAddress {
        input: args.mac.clone(),
        source,
    })?;

    let sender = match args.broadcast.as_deref() {
        Some(addr) => {
            let addr: SocketAddr = addr.parse().map_err(|_| CliError::BroadcastAddress {
                input: addr.to_string(),
            })?;
            WolSender::with_broadcast_addr(addr)
        }
        None => WolSender::new().map_err(|error| CliError::from(map::power(&error)))?,
    };

    sender
        .wake(&mac)
        .await
        .map_err(|error| CliError::from(map::power(&error)))?;
    println!("Sent a Wake-on-LAN magic packet to {mac}.");
    Ok(())
}

/// `tv-rs power-off <IP> --vendor <samsung|lg|roku>`
async fn run_power_off(args: PowerOffArgs) -> Result<(), CliError> {
    let client = TvClient::from_discovered(&discovered(args.vendor.into(), &args.ip))?;
    client.power_off().await?;
    println!(
        "Powered off the {} TV at {}.",
        client.vendor().as_str(),
        args.ip
    );
    Ok(())
}

/// `tv-rs key <IP> --vendor <samsung|roku> --key <KEY>`
async fn run_key(args: KeyArgs) -> Result<(), CliError> {
    match args.vendor {
        Vendor::Samsung => {
            let key = parse_tizen_key(&args.key).ok_or_else(|| CliError::UnknownKey {
                vendor: "samsung",
                name: args.key.clone(),
                valid: tizen_key_names(),
            })?;

            let client = TizenClient::new(args.ip.as_str());
            let mut session = TizenSession::connect(&client)
                .await
                .map_err(|error| CliError::from(map::tizen(&error, Capability::Key)))?;
            session
                .send_key(key)
                .await
                .map_err(|error| CliError::from(map::tizen(&error, Capability::Key)))?;

            println!("Sent the samsung key '{}' to {}.", args.key, args.ip);
            Ok(())
        }
        Vendor::Roku => {
            let key = parse_roku_key(&args.key).ok_or_else(|| CliError::UnknownKey {
                vendor: "roku",
                name: args.key.clone(),
                valid: roku_key_names(),
            })?;

            let client = RokuClient::new(args.ip.as_str());
            client
                .send_keypress(key)
                .await
                .map_err(|error| CliError::from(map::roku(&error, Capability::Key)))?;

            println!("Sent the roku key '{}' to {}.", args.key, args.ip);
            Ok(())
        }
        Vendor::Lg => Err(CliError::Unsupported {
            message: format!(
                "LG webOS has no remote-keypress API over SSAP, so key '{}' cannot be sent to {}",
                args.key, args.ip
            ),
        }),
    }
}

/// `tv-rs launch <IP> --vendor <samsung|lg|roku> --app <APP_ID>`
async fn run_launch(args: LaunchArgs) -> Result<(), CliError> {
    let client = TvClient::from_discovered(&discovered(args.vendor.into(), &args.ip))?;
    client.launch_app(&args.app).await?;
    println!(
        "Requested app '{}' on the {} TV at {}.",
        args.app,
        client.vendor().as_str(),
        args.ip
    );
    Ok(())
}

/// `tv-rs status [--timeout <SECONDS>]`
///
/// Discovery plus a power probe plus the capability set. This is the table a
/// technician reads when a screen misbehaves: it says what the crate can and cannot
/// do for each device, instead of leaving them to guess from the vendor.
async fn run_status(args: StatusArgs) -> Result<(), CliError> {
    let found = discover_all(args.timeout).await?;

    if found.is_empty() {
        println!("No televisions answered SSDP. Nothing was probed.");
        return Ok(());
    }

    let mut rows = Vec::with_capacity(found.len());
    for tv in &found {
        let capabilities = match TvClient::from_discovered(tv) {
            Ok(client) => {
                use tv_rs::core::DeviceHandle;
                client
                    .capabilities()
                    .iter()
                    .map(|capability| capability.as_str().to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            }
            Err(_) => "-".to_string(),
        };

        let (power, confidence) = match TvClient::from_discovered(tv) {
            Ok(client) => match client.power_observation().await {
                // The confidence is part of the answer, not a decoration: a probe
                // cannot tell "standby" from "on but nothing playing".
                Ok(observed) => (
                    observed
                        .value
                        .map(|state| format!("{state:?}"))
                        .unwrap_or_else(|| "Unknown".to_string()),
                    observed.confidence.as_str().to_string(),
                ),
                Err(error) => (format!("probe failed: {error}"), "unknown".to_string()),
            },
            Err(error) => (format!("no client: {error}"), "unknown".to_string()),
        };

        rows.push(render::StatusRow {
            vendor: tv.vendor.as_str().to_string(),
            label: tv.label().to_string(),
            address: tv.ip.clone(),
            power,
            confidence,
            capabilities,
        });
    }

    print!("{}", render::render_status_table(&rows));
    Ok(())
}

/// `tv-rs ensure --config <FILE> [--audit <FILE>]`
///
/// One reconciliation pass over the discovered fleet. Deliberately `--once`: a
/// long-running loop belongs in the agent's own binary, not in a CLI that a script
/// calls and waits on.
async fn run_ensure(args: EnsureArgs) -> Result<(), CliError> {
    use std::sync::Arc;

    use tv_rs::core::DeviceHandle;
    use tv_rs_agent::{AuditLog, Reconciler, StaticFileProvider};

    let found = discover_all(args.timeout).await?;
    if found.is_empty() {
        println!("No televisions answered SSDP. Nothing to reconcile.");
        return Ok(());
    }

    let mut fleet: Vec<Arc<dyn DeviceHandle>> = Vec::with_capacity(found.len());
    for tv in &found {
        match TvClient::from_discovered(tv) {
            Ok(client) => fleet.push(Arc::new(client)),
            Err(error) => println!("skipping {}: {error}", tv.label()),
        }
    }

    if fleet.is_empty() {
        println!("Discovered devices, but none had a usable control client.");
        return Ok(());
    }

    let provider = Arc::new(StaticFileProvider::new(&args.config));
    let mut reconciler = Reconciler::new(fleet, provider);
    if let Some(path) = &args.audit {
        reconciler = reconciler.with_audit(AuditLog::new(path));
    }

    let outcomes = reconciler.tick().await;
    print!("{}", render::render_outcomes(&outcomes));

    let failed = outcomes
        .iter()
        .filter(|outcome| outcome.result == "failed")
        .count();
    if failed > 0 {
        return Err(CliError::Agent(format!(
            "{failed} device(s) failed; see the table above"
        )));
    }
    Ok(())
}

/// Runs discovery with an optional timeout override.
async fn discover_all(timeout: Option<u64>) -> Result<Vec<DiscoveredTv>, CliError> {
    let found = match timeout {
        Some(seconds) => {
            let scanner = SsdpScanner::new().with_timeout(Duration::from_secs(seconds));
            tv_rs::discover_with(&scanner).await?
        }
        None => tv_rs::discover().await?,
    };

    // Loopback responders are the host's own UPnP service, not televisions.
    Ok(found
        .into_iter()
        .filter(|tv| !tv.ip.starts_with("127.") && tv.ip != "::1")
        .collect())
}
