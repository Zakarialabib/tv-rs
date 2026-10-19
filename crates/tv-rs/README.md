# tv-rs

Cross-platform control of televisions on the LAN: SSDP discovery, Wake-on-LAN, and
the Samsung Tizen, LG webOS and Roku ECP protocols behind one facade.

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

- `tv-rs` — this facade: `TvClient`, discovery, vendor dispatch
- `tv-rs-core` — the device model: capability traits, `DeviceHandle`, confidence, error taxonomy
- `tv-rs-discovery`, `tv-rs-power` — SSDP and Wake-on-LAN
- `tv-rs-tizen`, `tv-rs-webos`, `tv-rs-roku` — the protocol backends
- `tv-rs-agent` — desired-state reconciliation, healing ladder, audit log
- `tv-rs-cec` — HDMI-CEC for panels the network cannot reach
- `tv-rs-media` — `206`/HLS serving

Documentation, including what is deliberately unsupported, lives in the repository
`docs/` directory. Every deferred capability returns a typed error rather than
pretending to work.
