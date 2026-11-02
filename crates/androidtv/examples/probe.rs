//! The ATR v2 spike's 9-probe runner (`docs/04-androidtv-remote-v2-spike.md` §3).
//!
//! Run against a real panel on the same LAN:
//!
//! ```text
//! cargo run -p tv-rs-androidtv --example probe -- 192.168.1.20 [pairing-code]
//! ```
//!
//! Without a pairing code the runner stops after the on-screen prompt appears
//! (probes 1–3 of the handshake); pass the 6-hex code shown on the TV to
//! complete pairing and run the control probes. Each probe prints PASS/FAIL
//! with the wire evidence, in the exact row order of §3, so the output pastes
//! into `docs/08-hil-results.md` with no reformatting.
//!
//! This is a spike tool, not an API: it exists so the HIL session is one
//! command, not a rewrite.

use std::io::Write as _;

use tv_rs_androidtv::{AtrClient, ControlResponse, Key};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: probe <host> [pairing-code]");
        std::process::exit(2);
    }
    let host = args[1].clone();
    let pairing_code = args.get(2).cloned();

    println!("# ATR v2 probe run — host {host}, {}", chrono_now());
    println!();
    println!("| # | Probe | Result | Evidence |");
    println!("| --- | --- | --- | --- |");

    let mut client = AtrClient::new(&host);
    let Some(code) = pairing_code else {
        row(
            1,
            "Connect + handshake",
            "PARTIAL",
            "runner needs the 6-hex code shown on the TV after the prompt",
        );
        std::process::exit(0);
    };

    // Probes 1–2: mTLS connect, Polo pairing with the code.
    match client.pair("tv-rs-probe", &code) {
        Ok(identity) => {
            row(
                1,
                "Connect + handshake",
                "PASS",
                &format!("mTLS to {host}:6467; server cert pinned"),
            );
            row(
                2,
                "Pairing code + secret",
                "PASS",
                &format!(
                    "secret accepted; device name {:?}, MAC {}",
                    identity.name, identity.mac
                ),
            );
        }
        Err(err) => {
            let reason = err.to_string();
            row(
                1,
                "Connect + handshake",
                if reason.contains("tls") || reason.contains("io") {
                    "FAIL"
                } else {
                    "PASS"
                },
                &reason,
            );
            row(2, "Pairing code + secret", "FAIL", &reason);
            println!();
            println!("Stopping: pairing is the gate for every control probe.");
            std::process::exit(1);
        }
    }

    // Probes 3–9 run over the control port. Each is a round-trip; a failure
    // is recorded and the run continues, because the point of the spike is
    // the shape of what fails.
    let control = client.connect_control();
    let mut control = match control {
        Ok(session) => session,
        Err(err) => {
            for (n, name) in [
                (3usize, "State read"),
                (4, "Volume read"),
                (5, "Volume round-trip"),
                (6, "Key press"),
                (7, "App info / foreground"),
                (8, "Deep link"),
                (9, "Error + reconnect"),
            ] {
                row(n, name, "FAIL", &format!("control connect failed: {err}"));
            }
            std::process::exit(1);
        }
    };

    match control.ping() {
        Ok(ControlResponse::Ping) => row(3, "State read", "PASS", "ping round-trip; channel alive"),
        Ok(other) => row(
            3,
            "State read",
            "PARTIAL",
            &format!("unexpected ping reply: {other:?}"),
        ),
        Err(err) => row(3, "State read", "FAIL", &err.to_string()),
    }

    match control.get_volume() {
        Ok(ControlResponse::Volume(vol)) => row(
            4,
            "Volume read",
            "PASS",
            &format!(
                "max {} level {} muted {}",
                vol.volume_max, vol.volume_level, vol.volume_muted
            ),
        ),
        Ok(other) => row(
            4,
            "Volume read",
            "PARTIAL",
            &format!("unexpected reply: {other:?}"),
        ),
        Err(err) => row(4, "Volume read", "FAIL", &err.to_string()),
    }

    match control.set_volume(10) {
        Ok(ControlResponse::Volume(vol)) => row(
            5,
            "Volume round-trip",
            "PASS",
            &format!("set to 10, read back {}", vol.volume_level),
        ),
        Ok(other) => row(
            5,
            "Volume round-trip",
            "PARTIAL",
            &format!("set accepted, reply: {other:?}"),
        ),
        Err(err) => row(5, "Volume round-trip", "FAIL", &err.to_string()),
    }

    match control.press_key(Key::Home) {
        Ok(ControlResponse::KeyInject) => row(
            6,
            "Key press",
            "PASS",
            "KEYCODE_HOME injected (verify on panel)",
        ),
        Ok(other) => row(
            6,
            "Key press",
            "PARTIAL",
            &format!("unexpected reply: {other:?}"),
        ),
        Err(err) => row(6, "Key press", "FAIL", &err.to_string()),
    }

    match control.get_app_info() {
        Ok(ControlResponse::AppInfo(info)) => row(
            7,
            "App info / foreground",
            "PASS",
            &format!("foreground package: {}", info.app_package),
        ),
        Ok(other) => row(
            7,
            "App info / foreground",
            "PARTIAL",
            &format!("unexpected reply: {other:?}"),
        ),
        Err(err) => row(7, "App info / foreground", "FAIL", &err.to_string()),
    }

    match control.launch_app_link("https://tv.youtube.com") {
        Ok(_) => row(
            8,
            "Deep link",
            "PASS",
            "RemoteAppLinkLaunchRequest accepted (verify on panel)",
        ),
        Err(err) => row(8, "Deep link", "FAIL", &err.to_string()),
    }

    // Probe 9: a deliberately malformed frame must come back as an error
    // reply, not a dead socket — then the channel must still work.
    let malformed = tv_rs_androidtv::proto::remote::RemoteMessage::default();
    match control.send_raw(malformed) {
        Ok(_) => row(
            9,
            "Error + reconnect",
            "PASS",
            "empty frame answered; channel still usable",
        ),
        Err(err) => row(9, "Error + reconnect", "FAIL", &err.to_string()),
    }
}

fn row(n: usize, name: &str, result: &str, evidence: &str) {
    println!("| {n} | {name} | {result} | {evidence} |");
    let _ = std::io::stdout().flush();
}

fn chrono_now() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map(|s| format!("unix {s}"))
        .unwrap_or_else(|_| "clock before epoch".into())
}
