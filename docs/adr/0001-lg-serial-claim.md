# LG Serial Command Restrictions

**Verdict:** UNCONFIRMED (structurally inapplicable)

## The Claim
A circulating claim states that LG webOS firmware rejects all SSAP serial-channel commands except Power (`ka`) and Key (`mc`).

## Investigation
1. **Protocol Confusion:** The claim conflates two different LG protocols.
   - **SSAP** (Second Screen Application Protocol) is a JSON-RPC-like WebSocket protocol (port 3000/3001) used by the LG ThinQ app and `tv-rs-webos`.
   - **LG Serial/IP Control** is an ASCII-based protocol (RS-232 or TCP 9761) that uses 2-byte command headers like `ka` (Power) and `mc` (Key/Remote control).

2. **Crate Scope:** The `tv-rs-webos` crate implements the SSAP WebSocket protocol. It does not implement the ASCII serial protocol, so there are no "SSAP serial-channel commands" to restrict.

3. **Backend Status:** The ASCII serial protocol (`ka`, `mc`) is planned for a future `tv-rs-lgcmd` crate (as documented in `docs/06-plan.md`), but is not currently implemented in this workspace.

## Conclusion
Because the `tv-rs-webos` crate does not implement the ASCII serial protocol, and the `tv-rs-lgcmd` crate does not yet exist, there is no backend to apply a workaround to. According to `QUIRKS.md`, a row for a workaround with nothing to apply it to is banned. Therefore, no row has been added to `quirks/default.toml`.
