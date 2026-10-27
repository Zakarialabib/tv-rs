//! The command-line surface: clap argument definitions and the pure
//! string-to-key parsers they feed.
//!
//! Nothing here performs I/O. `clap` builds the [`Cli`] tree, and the
//! `parse_*`/`*_key_names` helpers translate the CLI's key spellings into the
//! protocol crates' enums and back. They are pure, so they are unit-tested
//! directly (§5, "pure core, thin transport").

use clap::{Args, Parser, Subcommand, ValueEnum};

use tv_rs::roku::RokuKey;
use tv_rs::tizen::TizenRemoteKey;
use tv_rs::TvVendor;

/// The `tv-rs` command-line interface.
#[derive(Debug, Parser)]
#[command(
    name = "tv-rs",
    version,
    about = "Discover, wake and control televisions on the local network."
)]
pub struct Cli {
    /// The subcommand to run.
    #[command(subcommand)]
    pub command: Command,
}

/// The operations the CLI exposes.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Discover televisions with SSDP and print an aligned table.
    Discover(DiscoverArgs),
    /// Wake a device by broadcasting a Wake-on-LAN magic packet.
    Wake(WakeArgs),
    /// Power a television off through the facade's vendor dispatch.
    #[command(name = "power-off")]
    PowerOff(PowerOffArgs),
    /// Send a remote-control key.
    Key(KeyArgs),
    /// Launch an installed application by id.
    Launch(LaunchArgs),
    /// Discover devices and print their power state and capabilities.
    Status(StatusArgs),
    /// Converge a fleet towards a desired-state file.
    Ensure(EnsureArgs),
    /// Universal IR Remote brand detection scanner using feedback loops.
    #[command(name = "ir-scan")]
    IrScan(IrScanArgs),
}

/// Arguments for `ir-scan`.
#[derive(Debug, Args)]
pub struct IrScanArgs {
    /// Target IP address for TCP power probe feedback verification.
    #[arg(long)]
    pub ip: Option<String>,
    /// Save matched IR profile to a TOML file path.
    #[arg(long)]
    pub output: Option<String>,
    /// Dry run mode using mock emitter.
    #[arg(long)]
    pub dry_run: bool,
}

/// Arguments for `status`.
#[derive(Debug, Args)]
pub struct StatusArgs {
    /// Seconds to wait for SSDP replies; omit to use the scanner's own default.
    #[arg(long)]
    pub timeout: Option<u64>,
}

/// Arguments for `ensure`.
#[derive(Debug, Args)]
pub struct EnsureArgs {
    /// JSON file mapping device id to desired state.
    #[arg(long, required_unless_present = "mqtt_broker")]
    pub config: Option<String>,
    /// Seconds to wait for SSDP replies; omit to use the scanner's own default.
    #[arg(long)]
    pub timeout: Option<u64>,
    /// Append every action to this JSONL file.
    #[arg(long)]
    pub audit: Option<String>,
    /// MQTT broker host:port.
    #[arg(long = "mqtt-broker", env = "MQTT_BROKER")]
    pub mqtt_broker: Option<String>,
    /// MQTT topic prefix.
    #[arg(long = "mqtt-prefix", env = "MQTT_PREFIX", default_value = "screens")]
    pub mqtt_prefix: String,
    /// Path to custom CA certificate for MQTT TLS.
    #[arg(long = "mqtt-ca-cert", env = "MQTT_CA_CERT")]
    pub mqtt_ca_cert: Option<String>,
    /// Path to client certificate for MQTT mTLS.
    #[arg(long = "mqtt-client-cert", env = "MQTT_CLIENT_CERT")]
    pub mqtt_client_cert: Option<String>,
    /// Path to client key for MQTT mTLS.
    #[arg(long = "mqtt-client-key", env = "MQTT_CLIENT_KEY")]
    pub mqtt_client_key: Option<String>,
    /// Allow plain TCP for MQTT connection.
    #[arg(long = "mqtt-allow-insecure", env = "MQTT_ALLOW_INSECURE")]
    pub mqtt_allow_insecure: bool,
}

/// Arguments for `discover`.
#[derive(Debug, Args)]
pub struct DiscoverArgs {
    /// Seconds to wait for SSDP replies; omit to use the scanner's own default.
    #[arg(long)]
    pub timeout: Option<u64>,
}

/// Arguments for `wake`.
#[derive(Debug, Args)]
pub struct WakeArgs {
    /// MAC address of the target device, e.g. `AA:BB:CC:DD:EE:FF`.
    pub mac: String,
    /// Broadcast address to send to, e.g. `192.168.1.255:9`.
    #[arg(long)]
    pub broadcast: Option<String>,
}

/// Arguments for `power-off`.
#[derive(Debug, Args)]
pub struct PowerOffArgs {
    /// IP address or hostname of the television.
    pub ip: String,
    /// Television vendor.
    #[arg(long, value_enum)]
    pub vendor: Vendor,
}

/// Arguments for `key`.
#[derive(Debug, Args)]
pub struct KeyArgs {
    /// IP address or hostname of the television.
    pub ip: String,
    /// Television vendor. `lg` is accepted so the CLI can report the gap.
    #[arg(long, value_enum)]
    pub vendor: Vendor,
    /// Remote-key spelling, e.g. `home`, `volume-up`, `power-off`.
    #[arg(long)]
    pub key: String,
}

/// Arguments for `launch`.
#[derive(Debug, Args)]
pub struct LaunchArgs {
    /// IP address or hostname of the television.
    pub ip: String,
    /// Television vendor.
    #[arg(long, value_enum)]
    pub vendor: Vendor,
    /// Vendor-specific application id to launch.
    #[arg(long)]
    pub app: String,
}

/// A television vendor as spelled on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Vendor {
    /// Samsung Tizen.
    Samsung,
    /// LG webOS.
    Lg,
    /// Roku OS.
    Roku,
}

impl From<Vendor> for TvVendor {
    fn from(vendor: Vendor) -> Self {
        match vendor {
            Vendor::Samsung => TvVendor::Samsung,
            Vendor::Lg => TvVendor::Lg,
            Vendor::Roku => TvVendor::Roku,
        }
    }
}

/// Every CLI spelling for a [`TizenRemoteKey`], paired with the key it selects.
const TIZEN_KEY_SPELLINGS: &[(&str, TizenRemoteKey)] = &[
    ("power", TizenRemoteKey::Power),
    ("power-off", TizenRemoteKey::PowerOff),
    ("home", TizenRemoteKey::Home),
    ("up", TizenRemoteKey::Up),
    ("down", TizenRemoteKey::Down),
    ("left", TizenRemoteKey::Left),
    ("right", TizenRemoteKey::Right),
    ("enter", TizenRemoteKey::Enter),
    ("return", TizenRemoteKey::Return),
    ("volume-up", TizenRemoteKey::VolumeUp),
    ("volume-down", TizenRemoteKey::VolumeDown),
    ("mute", TizenRemoteKey::Mute),
    ("channel-up", TizenRemoteKey::ChannelUp),
    ("channel-down", TizenRemoteKey::ChannelDown),
    ("menu", TizenRemoteKey::Menu),
    ("source", TizenRemoteKey::Source),
    ("play", TizenRemoteKey::Play),
    ("pause", TizenRemoteKey::Pause),
    ("stop", TizenRemoteKey::Stop),
    ("rewind", TizenRemoteKey::Rewind),
    ("fast-forward", TizenRemoteKey::FastForward),
];

/// Every CLI spelling for a [`RokuKey`], paired with the key it selects.
const ROKU_KEY_SPELLINGS: &[(&str, RokuKey)] = &[
    ("home", RokuKey::Home),
    ("rev", RokuKey::Rev),
    ("fwd", RokuKey::Fwd),
    ("play", RokuKey::Play),
    ("select", RokuKey::Select),
    ("left", RokuKey::Left),
    ("right", RokuKey::Right),
    ("down", RokuKey::Down),
    ("up", RokuKey::Up),
    ("back", RokuKey::Back),
    ("instant-replay", RokuKey::InstantReplay),
    ("info", RokuKey::Info),
    ("backspace", RokuKey::Backspace),
    ("search", RokuKey::Search),
    ("enter", RokuKey::Enter),
    ("power-off", RokuKey::PowerOff),
    ("power-on", RokuKey::PowerOn),
    ("volume-down", RokuKey::VolumeDown),
    ("volume-up", RokuKey::VolumeUp),
    ("volume-mute", RokuKey::VolumeMute),
    ("channel-up", RokuKey::ChannelUp),
    ("channel-down", RokuKey::ChannelDown),
];

/// Parse a CLI spelling into a [`TizenRemoteKey`].
///
/// Matching is case-insensitive and whitespace-trimmed. Returns `None` for an
/// unknown spelling rather than panicking; callers turn that into
/// [`CliError::UnknownKey`](crate::error::CliError::UnknownKey).
pub fn parse_tizen_key(name: &str) -> Option<TizenRemoteKey> {
    let normalized = name.trim().to_ascii_lowercase();
    TIZEN_KEY_SPELLINGS
        .iter()
        .find(|(spelling, _)| *spelling == normalized)
        .map(|(_, key)| *key)
}

/// Parse a CLI spelling into a [`RokuKey`].
///
/// Matching is case-insensitive and whitespace-trimmed. Returns `None` for an
/// unknown spelling rather than panicking.
pub fn parse_roku_key(name: &str) -> Option<RokuKey> {
    let normalized = name.trim().to_ascii_lowercase();
    ROKU_KEY_SPELLINGS
        .iter()
        .find(|(spelling, _)| *spelling == normalized)
        .map(|(_, key)| *key)
}

/// A comma-separated list of every accepted Tizen key spelling.
pub fn tizen_key_names() -> String {
    TIZEN_KEY_SPELLINGS
        .iter()
        .map(|(spelling, _)| *spelling)
        .collect::<Vec<_>>()
        .join(", ")
}

/// A comma-separated list of every accepted Roku key spelling.
pub fn roku_key_names() -> String {
    ROKU_KEY_SPELLINGS
        .iter()
        .map(|(spelling, _)| *spelling)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn parses_every_tizen_key_name_the_enum_defines() {
        for (spelling, expected) in TIZEN_KEY_SPELLINGS {
            assert_eq!(
                parse_tizen_key(spelling),
                Some(*expected),
                "spelling '{spelling}' should map to {expected:?}"
            );
        }
    }

    #[test]
    fn parses_every_roku_key_name_the_enum_defines() {
        for (spelling, expected) in ROKU_KEY_SPELLINGS {
            assert_eq!(
                parse_roku_key(spelling),
                Some(*expected),
                "spelling '{spelling}' should map to {expected:?}"
            );
        }
    }

    #[test]
    fn every_key_spelling_maps_to_a_distinct_key() {
        let tizen: HashSet<_> = TIZEN_KEY_SPELLINGS.iter().map(|(_, key)| *key).collect();
        assert_eq!(
            tizen.len(),
            TIZEN_KEY_SPELLINGS.len(),
            "two Tizen spellings must not map to the same key"
        );

        let roku: HashSet<_> = ROKU_KEY_SPELLINGS.iter().map(|(_, key)| *key).collect();
        assert_eq!(
            roku.len(),
            ROKU_KEY_SPELLINGS.len(),
            "two Roku spellings must not map to the same key"
        );
    }

    #[test]
    fn unknown_key_names_report_the_valid_set() {
        assert_eq!(parse_tizen_key("nonexistent"), None);
        assert_eq!(parse_roku_key("nonexistent"), None);

        assert!(tizen_key_names().contains("power-off"));
        assert!(roku_key_names().contains("instant-replay"));
    }

    #[test]
    fn key_spellings_are_case_insensitive_and_trimmed() {
        assert_eq!(parse_tizen_key("HOME"), parse_tizen_key("home"));
        assert_eq!(parse_tizen_key("  home  "), parse_tizen_key("home"));
        assert_eq!(parse_roku_key("POWER-OFF"), parse_roku_key("power-off"));
    }

    #[test]
    fn vendors_map_to_the_facade_vendor_enum() {
        assert_eq!(TvVendor::from(Vendor::Samsung), tv_rs::TvVendor::Samsung);
        assert_eq!(TvVendor::from(Vendor::Lg), tv_rs::TvVendor::Lg);
        assert_eq!(TvVendor::from(Vendor::Roku), tv_rs::TvVendor::Roku);
    }
}
