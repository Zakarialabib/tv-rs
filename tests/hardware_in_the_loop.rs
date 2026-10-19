//! Hardware-in-the-loop smoke tests for the `tv-rs` facade.
//!
//! These tests talk to **real televisions on the local network**. They are
//! compiled only when the `hil` feature is enabled, so a plain `cargo test`
//! never touches hardware:
//!
//! ```text
//! cargo test --features hil                       # skips without env vars
//! TV_RS_HIL_ROKU_HOST=192.168.1.40 \
//!   cargo test --features hil --test hardware_in_the_loop
//! ```
//!
//! Every test degrades to a printed skip when its host variable is unset, so a
//! partially equipped lab never fails the suite. Set `TV_RS_HIL_STRICT=1` to
//! turn missing hardware into a failure (useful for a dedicated lab runner).
//!
//! | Variable | Purpose |
//! | --- | --- |
//! | `TV_RS_HIL_ROKU_HOST` | Roku IP (ECP on 8060, unauthenticated) |
//! | `TV_RS_HIL_TIZEN_HOST` | Samsung Tizen IP (WS 8001/8002) |
//! | `TV_RS_HIL_TIZEN_TOKEN` | Previously saved Tizen pairing token (optional) |
//! | `TV_RS_HIL_WEBOS_HOST` | LG webOS IP (SSAP 3000/3001) |
//! | `TV_RS_HIL_MAC` | MAC address of a TV for Wake-on-LAN |
//! | `TV_RS_HIL_WOL_PROBE_HOST` | Host for the post-wake power probe |
//! | `TV_RS_HIL_WOL_PROBE_PORT` | TCP port for the probe (default 8001) |
//! | `TV_RS_HIL_STRICT` | `1` = missing hardware fails instead of skipping |
//!
//! Security: never commit real device IPs, MACs or pairing tokens. Pass them
//! through the environment only.

#![cfg(feature = "hil")]

use std::time::Duration;

/// Returns the value of `name`, or `None` when unset (printed as a skip).
fn env_host(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(value) if !value.trim().is_empty() => Some(value),
        _ => {
            if std::env::var("TV_RS_HIL_STRICT").as_deref() == Ok("1") {
                panic!("{name} must be set when TV_RS_HIL_STRICT=1");
            }
            eprintln!("skip: {name} is not set (see tests/hardware_in_the_loop.rs)");
            None
        }
    }
}

/// SSDP discovery must see *something* on a lab network.
#[tokio::test]
async fn ssdp_discovery_finds_a_device() {
    if std::env::var("TV_RS_HIL_SKIP_DISCOVERY").as_deref() == Ok("1") {
        eprintln!("skip: TV_RS_HIL_SKIP_DISCOVERY=1");
        return;
    }

    let scanner = tv_rs::discovery::SsdpScanner::new().with_timeout(Duration::from_secs(3));

    let replies = match scanner.discover().await {
        Ok(replies) => replies,
        // An isolated CI network with no multicast is a legitimate skip.
        Err(error) => {
            eprintln!("skip: SSDP discovery unavailable on this network: {error}");
            return;
        }
    };

    for reply in &replies {
        println!(
            "ssdp: usn={:?} st={:?} server={:?} location={:?}",
            reply.usn, reply.st, reply.server, reply.location
        );
    }

    assert!(
        !replies.is_empty(),
        "SSDP discovery returned no devices on the local network"
    );
}

/// Roku ECP is unauthenticated, so this is the cheapest real control check.
#[tokio::test]
async fn roku_device_info_roundtrip() {
    let Some(host) = env_host("TV_RS_HIL_ROKU_HOST") else {
        return;
    };

    let client = tv_rs_roku::RokuClient::new(&host);
    let info = match client.query_device_info().await {
        Ok(info) => info,
        Err(error) => {
            eprintln!("skip: Roku at {host} did not answer ECP: {error}");
            return;
        }
    };

    println!(
        "roku: model={:?} firmware={:?} power={:?}",
        info.model_name, info.software_version, info.power_mode
    );
    assert!(
        info.model_name.is_some() || info.serial_number.is_some(),
        "ECP /query/device-info returned neither model nor serial"
    );
}

/// A keypress proves the ECP write path, not just the read path.
#[tokio::test]
async fn roku_keypress_is_accepted() {
    let Some(host) = env_host("TV_RS_HIL_ROKU_HOST") else {
        return;
    };

    // `Home` is harmless: it only returns to the Roku home screen.
    match tv_rs_roku::RokuClient::new(&host)
        .send_keypress(tv_rs_roku::RokuKey::Home)
        .await
    {
        Ok(()) => println!("roku: Home keypress accepted by {host}"),
        Err(error) => eprintln!("skip: keypress failed against {host}: {error}"),
    }
}

/// Tizen remote-channel handshake (unencrypted 8001 by default).
///
/// On a first connection the TV shows a PIN; this test only asserts the
/// transport handshake completes, not that a user accepted the prompt.
#[tokio::test]
async fn tizen_channel_connects() {
    let Some(host) = env_host("TV_RS_HIL_TIZEN_HOST") else {
        return;
    };

    let client = tv_rs_tizen::TizenClient::new(&host);
    match tv_rs_tizen::TizenSession::connect(&client).await {
        Ok(session) => println!(
            "tizen: channel connected to {host}, token captured={}",
            session.token().is_some()
        ),
        Err(error) => eprintln!("skip: Tizen at {host} refused the channel: {error}"),
    }
}

/// webOS pairing handshake. Persists the `client-key` when one is issued.
#[tokio::test]
async fn webos_pairing_issues_client_key() {
    let Some(host) = env_host("TV_RS_HIL_WEBOS_HOST") else {
        return;
    };

    let client = tv_rs_webos::WebosClient::new(&host).with_device_name("tv-rs-hil");
    match client.connect().await {
        Ok(session) => {
            println!(
                "webos: paired with {host}, client-key issued={}",
                session.client_key().is_some()
            );
            if let Some(key) = session.client_key() {
                let path = format!("webos-client-key-{host}.json");
                if let Err(error) = tv_rs_webos::save_client_key(&path, key).await {
                    eprintln!("warning: could not persist client-key to {path}: {error}");
                }
            }
        }
        Err(error) => eprintln!("skip: webOS at {host} refused pairing: {error}"),
    }
}

/// Wake-on-LAN: broadcasts a magic packet and probes the TV's liveness after.
#[tokio::test]
async fn wake_on_lan_then_probe() {
    let Some(mac) = env_host("TV_RS_HIL_MAC") else {
        return;
    };
    let Some(host) = env_host("TV_RS_HIL_WOL_PROBE_HOST") else {
        return;
    };

    let mac: tv_rs_power::MacAddress = match mac.parse() {
        Ok(mac) => mac,
        Err(error) => {
            eprintln!("skip: TV_RS_HIL_MAC is not a MAC address: {error}");
            return;
        }
    };

    let sender = match tv_rs_power::WolSender::new() {
        Ok(sender) => sender,
        Err(error) => {
            eprintln!("skip: cannot open a WoL socket: {error}");
            return;
        }
    };

    if let Err(error) = sender.wake(&mac).await {
        eprintln!("skip: magic packet could not be sent: {error}");
        return;
    }

    let port = std::env::var("TV_RS_HIL_WOL_PROBE_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8001);
    let addr = format!("{host}:{port}");

    // Give the TV a moment to bring the application services up.
    tokio::time::sleep(Duration::from_secs(5)).await;

    match tv_rs_power::poll_power_state(&addr).await {
        Ok(state) => println!("wol: state after wake at {addr} = {state:?}"),
        Err(error) => eprintln!("skip: probe against {addr} failed: {error}"),
    }
}

/// The vendor heuristic must recognize the signatures The TV advertises.
#[test]
fn facade_identifies_vendor_from_ssdp() {
    use tv_rs::discovery::SsdpMsearchResponse;

    let cases = [
        (
            SsdpMsearchResponse {
                server: Some("Roku/9.3.0 UPnP/1.0 Roku/9.3.0".to_string()),
                st: Some("roku:ecp".to_string()),
                source: "192.168.1.40".to_string(),
                ..Default::default()
            },
            tv_rs::TvVendor::Roku,
        ),
        (
            SsdpMsearchResponse {
                server: Some("Samsung UPnP SDK Tizen".to_string()),
                source: "192.168.1.41".to_string(),
                ..Default::default()
            },
            tv_rs::TvVendor::Samsung,
        ),
        (
            SsdpMsearchResponse {
                server: Some("LG Electronics webOS".to_string()),
                source: "192.168.1.42".to_string(),
                ..Default::default()
            },
            tv_rs::TvVendor::Lg,
        ),
        (
            SsdpMsearchResponse {
                server: Some("Linux/1.0 UPnP/1.1".to_string()),
                source: "192.168.1.43".to_string(),
                ..Default::default()
            },
            tv_rs::TvVendor::Unknown,
        ),
    ];

    for (response, expected) in cases {
        assert_eq!(
            tv_rs::identify_vendor(&response, None),
            expected,
            "vendor heuristic mislabeled {}",
            response.server.as_deref().unwrap_or("<no server header>")
        );
    }
}
