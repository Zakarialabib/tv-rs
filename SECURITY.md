# Security policy

## Reporting a vulnerability

Do not open a public issue. Report privately through GitHub's
[Security Advisories](https://github.com/Zakarialabib/tv-rs/security/advisories/new)
so a fix can be prepared before disclosure.

Please include the affected crate and version, the protocol involved (SSDP,
Wake-on-LAN, Tizen, webOS, Roku, media server) and a reproduction. Expect an
acknowledgement within 72 hours.

## Scope

This workspace talks to devices on a local network and handles pairing
credentials. Reports in these areas are especially relevant:

- **Credential handling** — Tizen pairing tokens and webOS `client-key` values
  are written to paths chosen by the caller. A path-traversal, world-readable
  file mode (`client-key` is written with default permissions) or accidental
  logging of a token is in scope.
- **Media server** — `tv-rs-media` serves files from a root directory and must
  reject path traversal (`../`). A bypass of `safe_join`, or a range request
  that reads outside the requested file, is in scope.
- **Unsafe code** — `unsafe` is limited to the FFI surface; unsound blocks in
  FFI code are in scope.
- **Denial of service** — unbounded SSDP response parsing or frame budgets that
  a malicious LAN peer can exhaust.

## Out of scope

- Anything requiring physical access to a TV or its firmware.
- The reverse-engineering gaps documented in `docs/01-playbook.md` §10 — those
  features are deliberately unimplemented and fail closed.
- Vulnerabilities in the vendor SDK (`OpenNetStream.dll`); report those to the
  vendor. This workspace never redistributes it.
