# Quirks: submitting a row

`quirks/default.toml` is a table of **model- and firmware-specific defects with
their documented handling**. A backend consults it before attempting something that
is known to fail on that device.

It is data, not code, for one reason: a new quirk should be a review of a claim, not
a review of a pull request that touches five crates.

## Adding a row

```toml
[[quirk]]
vendor = "samsung"          # samsung | lg | roku
model_prefix = "QN90"       # optional; prefix match, e.g. QN90A/QN90B/QN90D
firmware_prefix = "T-KTM"   # optional; prefix match on the reported version
issue = "ws_8001_disabled"  # stable id, snake_case, no spaces
action = "start_at_8002"    # what the client should do instead
note = "one sentence, with the source"
```

Three requirements, enforced by review and partly by the parser:

1. **A source.** Vendor documentation, a firmware release note, or a behaviour you
   reproduced. `note` carries it. "Seen on a forum" is not enough; "seen on a forum,
   reproduced on a 2021 QN90A running 2105" is.
2. **A stable `issue` id.** It becomes part of the public error surface
   (`TvError::PlatformQuirk(QuirkId)`), so renaming one is a breaking change.
3. **Correct position.** **The file order is the precedence order** — the first
   matching row wins. Specific rows go above general ones; a general row placed
   first silently disables every workaround below it. A test pins this.

## What does not belong here

- **A vendor we do not drive.** Vizio SmartCast, Hisense VIDAA and Android TV have no
  backend yet; a row for them would be a workaround with nothing to apply it.
- **A bug in this workspace.** If the client is wrong, fix the client. The table is
  for televisions behaving differently from their documentation.
- **A guess.** An unverified row is worse than no row: it makes a device take a
  fallback path nobody has tested.

## Where it is applied

| Backend | Today | Next |
|---|---|---|
| Samsung | port ladder already tries 8002 → 8001 | skip 8001 outright when the quirk says it is dead |
| LG | port ladder already tries 3001 → 3000 | same |
| Roku | not consulted yet | map `keypress_gated` to `Auth(NotPermitted)` with the setting named, and read `supports-tv-power-control` from OS 15 |
