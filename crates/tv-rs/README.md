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
- `tv-rs-drivers` — every vendor protocol behind a feature flag: SSDP, Wake-on-LAN, Tizen, webOS, Roku, Google Cast, Samsung MDC, Philips SICP, HDMI-CEC, IR
- `tv-rs-lgcmd` — LG commercial serial (`ka`/`mc`)
- `tv-rs-agent` — desired-state reconciliation, lease handling, healing ladder, audit log
- `tv-rs-media` — `206`/HLS serving

The ten old `tv-rs-<vendor>` crate names are deprecated, unpublished shims that
re-export `tv-rs-drivers`; new code should depend on `tv-rs-drivers` with the
features it needs.

Documentation, including what is deliberately unsupported, lives in the repository `docs/` directory. Every deferred capability returns a typed error rather than pretending to work.
