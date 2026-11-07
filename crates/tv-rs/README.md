# tv-rs

Cross-platform control of televisions and smart displays on the LAN: SSDP/mDNS discovery, Wake-on-LAN, Samsung Tizen, LG webOS, Roku ECP, Google Cast, and HDMI-CEC protocols behind one small facade.

```rust,no_run
# async fn example() -> Result<(), tv_rs::TvError> {
let tvs = tv_rs::discover().await?;
if let Some(tv) = tvs.first() {
    let client = tv.to_client()?;
    println!("{}: {:?}", tv.label(), client.power_state().await?);
}
# Ok(())
# }
```

`discover_stream(scanner, on_found)` is the same scan with a callback per display, so a "Find displays" screen fills in as sets answer instead of waiting for the reply window to close.

`PowerControl::wake` owns the whole ladder: Roku ECP, falling back to a magic packet when the handle carries an address, and a packet for Tizen and webOS, which drop their network stack when they sleep. `TvClient::with_mac` is where that address comes from.

## Architecture

- `tv-rs` — this facade: `TvClient`, discovery, vendor dispatch
- `tv-rs-core` — the device model: capability traits, `DeviceHandle`, confidence, error taxonomy
- `tv-rs-discovery`, `tv-rs-power` — SSDP, mDNS, and Wake-on-LAN
- `tv-rs-tizen`, `tv-rs-webos`, `tv-rs-roku` — consumer smart TV backends
- `tv-rs-cast` — Google Cast sender with media transport controls
- `tv-rs-mdc`, `tv-rs-sicp`, `tv-rs-lgcmd` — commercial display protocols
- `tv-rs-cec` — HDMI-CEC over subprocess (`cec-client`) or platform-native FFI
- `tv-rs-agent` — desired-state reconciliation, lease handling, healing ladder, audit log
- `tv-rs-media` — `206`/HLS serving

Documentation, including what is deliberately unsupported, lives in the repository `docs/` directory. Every deferred capability returns a typed error rather than pretending to work.
