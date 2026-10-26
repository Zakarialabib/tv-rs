//! SSDP advertisement — the LAN-visibility half of a MediaServer.
//!
//! [`crate::discovery`] *asks* the LAN which renderers exist; this module makes
//! **us** answerable: periodic `ssdp:alive` NOTIFY broadcasts (so TVs and
//! controller apps see a MediaServer without any manual setup — LAN-visibility
//! without configuration, DLNA's core premise) plus a best-effort M-SEARCH
//! responder for devices that probe unicast-first.
//!
//! Design notes:
//! - Datagram construction is in pure functions ([`alive_datagrams`],
//!   [`search_response`]) so the protocol bytes are unit-testable without a
//!   socket.
//! - The multicast *listener* (port 1900) is optional by nature: another UPnP
//!   stack on the host may hold the port, and a failed bind must never take down
//!   the `alive` loop — discovery-by-alive is the primary path.
//! - `ssdp:byebye` is sent on [`AdvertiseHandle::stop`] so TVs drop us from
//!   their source lists when the server shuts down.
//!
//! This module is transport only: what is *advertised* (media, content, auth)
//! is the caller's service behind the `LOCATION` URL.

use std::time::Duration;

use tokio::net::UdpSocket;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

const SSDP_ADDR: &str = "239.255.255.250:1900";

/// The UPnP device type a MediaServer normally advertises.
pub const MEDIA_SERVER_DEVICE_TYPE: &str = "urn:schemas-upnp-org:device:MediaServer:1";

/// Everything needed to describe ourselves to the LAN.
#[derive(Debug, Clone)]
pub struct AdvertiseConfig {
    /// Full UDN including the `uuid:` prefix (e.g. `uuid:9d1f…`).
    pub udn: String,
    /// `LOCATION` URL — must be a LAN-reachable `description.xml`.
    pub location: String,
    /// `friendlyName` of the device (log/UX parity with the description doc).
    pub friendly_name: String,
    /// UPnP device type, normally [`MEDIA_SERVER_DEVICE_TYPE`].
    pub device_type: String,
    /// `SERVER` header value announced in both NOTIFY and M-SEARCH replies.
    ///
    /// Convention: `product/version UPnP/1.0 product/version` (the product may
    /// appear twice, mirroring many real stacks). Callers identify themselves;
    /// the crate stays vendor-neutral.
    pub server: String,
}

/// (NT, USN) pairs a MediaServer announces, per UPnP Device Architecture 1.0.
fn nt_usn_pairs(cfg: &AdvertiseConfig) -> Vec<(String, String)> {
    vec![
        (
            "upnp:rootdevice".to_string(),
            format!("{}::upnp:rootdevice", cfg.udn),
        ),
        (cfg.udn.clone(), cfg.udn.clone()),
        (
            cfg.device_type.clone(),
            format!("{}::{}", cfg.udn, cfg.device_type),
        ),
    ]
}

/// One `NOTIFY * HTTP/1.1` datagram body for the given `NT` (alive or byebye).
fn notify(cfg: &AdvertiseConfig, nts: &str, nt: &str, usn: &str) -> String {
    format!(
        "NOTIFY * HTTP/1.1\r\n\
         HOST: 239.255.255.250:1900\r\n\
         CACHE-CONTROL: max-age=1800\r\n\
         LOCATION: {location}\r\n\
         NT: {nt}\r\n\
         NTS: {nts}\r\n\
         SERVER: {server}\r\n\
         USN: {usn}\r\n\
         \r\n",
        location = cfg.location,
        nts = nts,
        nt = nt,
        server = cfg.server,
        usn = usn,
    )
}

/// Full set of `ssdp:alive` datagrams (rootdevice + uuid + device type).
pub fn alive_datagrams(cfg: &AdvertiseConfig) -> Vec<String> {
    nt_usn_pairs(cfg)
        .into_iter()
        .map(|(nt, usn)| notify(cfg, "ssdp:alive", &nt, &usn))
        .collect()
}

/// Full set of `ssdp:byebye` datagrams (sent twice on stop for reliability).
pub fn byebye_datagrams(cfg: &AdvertiseConfig) -> Vec<String> {
    nt_usn_pairs(cfg)
        .into_iter()
        .map(|(nt, usn)| notify(cfg, "ssdp:byebye", &nt, &usn))
        .collect()
}

/// Does this M-SEARCH `ST` warrant a unicast reply from us?
///
/// Mirrors the alive set: `ssdp:all` gets a reply, and the three concrete
/// targets (rootdevice, our UDN, our device type) get their own reply. We also
/// answer probes for [`MEDIA_SERVER_DEVICE_TYPE`] by name so generic DLNA
/// controller loops that search for exactly the MediaServer type find us.
///
/// Anything else (MediaRenderer probes, other vendors' device types, …) is
/// somebody else's device — answering those would confuse controllers.
pub fn should_respond(st: &str, cfg: &AdvertiseConfig) -> bool {
    st.eq_ignore_ascii_case("ssdp:all")
        || st.eq_ignore_ascii_case("upnp:rootdevice")
        || st.eq_ignore_ascii_case(&cfg.udn)
        || st.eq_ignore_ascii_case(&cfg.device_type)
        || st.eq_ignore_ascii_case(MEDIA_SERVER_DEVICE_TYPE)
}

/// Unicast `200 OK` response body for a matching M-SEARCH.
pub fn search_response(cfg: &AdvertiseConfig, st: &str) -> String {
    // For ssdp:all the spec wants one response per NT; collapsing to the
    // device type keeps the responder one-packet and every receiver happy
    // (controllers dedupe by USN anyway).
    let (resp_st, usn) = if st.eq_ignore_ascii_case("ssdp:all") {
        (
            cfg.device_type.clone(),
            format!("{}::{}", cfg.udn, cfg.device_type),
        )
    } else {
        (st.to_string(), format!("{}::{}", cfg.udn, st))
    };
    format!(
        "HTTP/1.1 200 OK\r\n\
         CACHE-CONTROL: max-age=1800\r\n\
         EXT:\r\n\
         LOCATION: {location}\r\n\
         SERVER: {server}\r\n\
         ST: {st}\r\n\
         USN: {usn}\r\n\
         \r\n",
        location = cfg.location,
        server = cfg.server,
        st = resp_st,
        usn = usn,
    )
}

/// Parse an incoming M-SEARCH into its `ST` (case-insensitive header scan;
/// lines without a colon — the request line — are skipped, same lesson as
/// [`crate::discovery`]'s SSDP `LOCATION` parsing).
pub fn parse_msearch_st(message: &str) -> Option<String> {
    let first_line = message.lines().next()?;
    if !first_line.starts_with("M-SEARCH") {
        return None;
    }
    for line in message.lines().skip(1) {
        if let Some((key, value)) = line.split_once(':') {
            if key.trim().eq_ignore_ascii_case("ST") {
                let value = value.trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// Handle to a running advertisement; [`stop`](AdvertiseHandle::stop) sends
/// byebye and halts tasks.
pub struct AdvertiseHandle {
    stop_tx: Option<oneshot::Sender<()>>,
    tasks: Vec<JoinHandle<()>>,
}

impl AdvertiseHandle {
    /// Send `ssdp:byebye` burst and stop both tasks. Idempotent.
    pub async fn stop(mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
        for task in self.tasks {
            let _ = task.await;
        }
    }
}

/// Spawn the advertisement for `cfg`.
///
/// - Immediately broadcasts the `ssdp:alive` set, then repeats every
///   `alive_interval` (well inside the 1800 s cache age).
/// - Best-effort M-SEARCH responder on UDP 1900 (skipped with a warning if
///   the port is taken — never fatal).
/// - On `stop()`: two `ssdp:byebye` bursts, then both tasks exit.
pub async fn advertise(cfg: AdvertiseConfig, alive_interval: Duration) -> AdvertiseHandle {
    let (stop_tx, stop_rx) = oneshot::channel::<()>();
    let mut tasks = Vec::new();

    // ── Task 1: periodic alive broadcast ─────────────────────────────────
    {
        let cfg_alive = cfg.clone();
        tasks.push(tokio::spawn(async move {
            let datagrams = alive_datagrams(&cfg_alive);
            let mut stop_rx = stop_rx;
            loop {
                send_multicast(&datagrams).await;
                tokio::select! {
                    _ = tokio::time::sleep(alive_interval) => {}
                    _ = &mut stop_rx => break,
                }
            }
            // Best-effort byebye so TVs drop us even if the caller's stop()
            // races the task shutdown path.
            send_multicast(&byebye_datagrams(&cfg_alive)).await;
        }));
    }

    // ── Task 2: best-effort M-SEARCH responder ───────────────────────────
    {
        let cfg_rsp = cfg;
        tasks.push(tokio::spawn(async move {
            let socket = match bind_multicast_listener().await {
                Some(s) => s,
                None => return, // port held by another UPnP stack — alive loop still runs
            };
            let mut buf = vec![0u8; 4096];
            while let Ok((len, peer)) = socket.recv_from(&mut buf).await {
                let text = String::from_utf8_lossy(&buf[..len]);
                if let Some(st) = parse_msearch_st(&text) {
                    if should_respond(&st, &cfg_rsp) {
                        let resp = search_response(&cfg_rsp, &st);
                        let _ = socket.send_to(resp.as_bytes(), peer).await;
                    }
                }
            }
        }));
    }

    AdvertiseHandle {
        stop_tx: Some(stop_tx),
        tasks,
    }
}

/// Bind UDP 1900 and join the SSDP multicast group (best-effort → `None`).
async fn bind_multicast_listener() -> Option<UdpSocket> {
    let socket = UdpSocket::bind("0.0.0.0:1900").await.ok()?;
    let group: std::net::Ipv4Addr = "239.255.255.250".parse().ok()?;
    socket
        .join_multicast_v4(group, std::net::Ipv4Addr::UNSPECIFIED)
        .ok()?;
    socket.set_multicast_loop_v4(true).ok()?;
    Some(socket)
}

async fn send_multicast(datagrams: &[String]) {
    let socket = match UdpSocket::bind("0.0.0.0:0").await {
        Ok(s) => s,
        Err(e) => {
            log::warn!("[tv-rs-dlna] cannot open socket for advertisement: {e}");
            return;
        }
    };
    let _ = socket.set_broadcast(true);
    let Ok(addr) = SSDP_ADDR.parse::<std::net::SocketAddr>() else {
        return;
    };
    for d in datagrams {
        if let Err(e) = socket.send_to(d.as_bytes(), addr).await {
            log::debug!("[tv-rs-dlna] notify send failed: {e}");
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> AdvertiseConfig {
        AdvertiseConfig {
            udn: "uuid:11111111-2222-3333-4444-555555555555".to_string(),
            location: "http://192.168.1.40:7411/description.xml".to_string(),
            friendly_name: "SignageMaster".to_string(),
            device_type: MEDIA_SERVER_DEVICE_TYPE.to_string(),
            server: "tv-rs-dlna/0.1.0 UPnP/1.0 tv-rs-dlna/0.1.0".to_string(),
        }
    }

    #[test]
    fn alive_datagrams_cover_the_three_nts() {
        let d = alive_datagrams(&cfg());
        assert_eq!(d.len(), 3);
        assert!(d.iter().all(|x| x.starts_with("NOTIFY * HTTP/1.1\r\n")));
        assert!(d.iter().all(|x| x.contains("NTS: ssdp:alive\r\n")));
        assert!(d.iter().any(|x| x.contains("NT: upnp:rootdevice\r\n")));
        assert!(d
            .iter()
            .any(|x| x.contains("NT: uuid:11111111-2222-3333-4444-555555555555\r\n")));
        assert!(d
            .iter()
            .any(|x| x.contains("NT: urn:schemas-upnp-org:device:MediaServer:1\r\n")));
        // Every datagram must be CRLF-terminated and carry LOCATION + HOST + SERVER.
        for x in &d {
            assert!(x.contains("HOST: 239.255.255.250:1900\r\n"));
            assert!(x.contains("LOCATION: http://192.168.1.40:7411/description.xml\r\n"));
            assert!(x.contains("SERVER: tv-rs-dlna/0.1.0 UPnP/1.0 tv-rs-dlna/0.1.0\r\n"));
            assert!(x.ends_with("\r\n\r\n"));
        }
    }

    #[test]
    fn byebye_uses_nts_byebye() {
        let d = byebye_datagrams(&cfg());
        assert_eq!(d.len(), 3);
        assert!(d.iter().all(|x| x.contains("NTS: ssdp:byebye\r\n")));
    }

    #[test]
    fn responds_to_matching_sts_only() {
        let c = cfg();
        assert!(should_respond("ssdp:all", &c));
        assert!(should_respond("upnp:rootdevice", &c));
        assert!(should_respond(
            "urn:schemas-upnp-org:device:MediaServer:1",
            &c
        ));
        assert!(should_respond("SSDP:ALL", &c)); // header match is case-insensitive
                                                 // Other vendors' device types are somebody else's device: answering those
                                                 // would confuse their controllers.
        assert!(!should_respond(
            "urn:samsung-com:device:RemoteControlReceiver:1",
            &c
        ));
        assert!(!should_respond(
            "urn:schemas-upnp-org:device:MediaRenderer:1",
            &c
        ));
        assert!(!should_respond(
            "urn:schemas-upnp-org:service:ContentDirectory:1",
            &c
        ));
    }

    #[test]
    fn responds_to_a_custom_device_type_and_udn() {
        // When the caller advertises a non-standard device_type/udn, those STs must
        // match through the config, not the const.
        let mut c = cfg();
        c.device_type = "urn:example-com:device:CustomSignage:2".to_string();
        assert!(should_respond("urn:example-com:device:CustomSignage:2", &c));
        assert!(should_respond(
            "uuid:11111111-2222-3333-4444-555555555555",
            &c
        ));
        // The MediaServer const branch stays: we ARE a MediaServer and generic
        // controllers search for exactly that ST.
        assert!(should_respond(
            "urn:schemas-upnp-org:device:MediaServer:1",
            &c
        ));
    }

    #[test]
    fn msearch_parse_skips_request_line_and_finds_st() {
        let msg = "M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\nMAN: \"ssdp:discover\"\r\nMX: 3\r\nST: ssdp:all\r\n\r\n";
        assert_eq!(parse_msearch_st(msg).as_deref(), Some("ssdp:all"));
        // Header scan is case-insensitive; empty ST is rejected.
        assert!(parse_msearch_st("M-SEARCH * HTTP/1.1\r\nst:\r\n").is_none());
        // Non-M-SEARCH traffic is ignored (our own NOTIFYs loop back with multicast
        // loop enabled).
        assert!(parse_msearch_st(&alive_datagrams(&cfg())[0]).is_none());
    }

    #[test]
    fn msearch_parse_handles_malformed_and_non_msearch_inputs() {
        // Non-M-SEARCH HTTP requests
        assert!(parse_msearch_st("GET /description.xml HTTP/1.1\r\nST: ssdp:all\r\n\r\n").is_none());
        assert!(parse_msearch_st("POST /event HTTP/1.1\r\nST: ssdp:all\r\n\r\n").is_none());

        // Missing ST header
        assert!(parse_msearch_st("M-SEARCH * HTTP/1.1\r\nHOST: 239.255.255.250:1900\r\n\r\n").is_none());

        // Whitespace only ST header value
        assert!(parse_msearch_st("M-SEARCH * HTTP/1.1\r\nST:    \r\n").is_none());

        // Malformed lines without colons
        assert!(parse_msearch_st("M-SEARCH * HTTP/1.1\r\nINVALID_HEADER_LINE\r\nST: ssdp:all\r\n").as_deref() == Some("ssdp:all"));

        // Case variation and extra whitespace trimming in header key and value
        assert_eq!(
            parse_msearch_st("M-SEARCH * HTTP/1.1\r\nSt:   upnp:rootdevice   \r\n").as_deref(),
            Some("upnp:rootdevice")
        );
    }

    #[test]
    fn search_response_carries_usn_pairing_udn_and_st() {
        let r = search_response(&cfg(), "urn:schemas-upnp-org:device:MediaServer:1");
        assert!(r.starts_with("HTTP/1.1 200 OK\r\n"));
        assert!(r.contains("ST: urn:schemas-upnp-org:device:MediaServer:1\r\n"));
        assert!(r.contains("USN: uuid:11111111-2222-3333-4444-555555555555::urn:schemas-upnp-org:device:MediaServer:1\r\n"));
        assert!(r.ends_with("\r\n\r\n"));

        let all = search_response(&cfg(), "ssdp:all");
        // ssdp:all collapses to the device type, not literal "ssdp:all".
        assert!(all.contains("ST: urn:schemas-upnp-org:device:MediaServer:1\r\n"));
        assert!(!all.contains("ST: ssdp:all\r\n"));
    }
}
