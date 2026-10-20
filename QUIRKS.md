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

## The action vocabulary

`action` is not prose: `tv-rs-core` decodes it into `QuirkAction`, and a row whose
action this crate cannot act on fails a test. Current vocabulary:

| `action` | Means | Applied by |
|---|---|---|
| `start_at_8001` / `start_at_8002` | only this port exists on the device | facade pins the port (`TizenClient::pinned_port`) |
| `start_at_3000` / `start_at_3001` | same, for webOS | facade pins the port (`WebosClient::pinned_port`) |
| `auth_not_permitted` | the device refuses until a setting is enabled | surfaced as `Auth(NotPermitted)`, never retried |
| `read_device_info_flags` | trust the device-info flags over the model | the Roku client reads them |
| `point_user_at_settings` | a human has to change something | reported, with the setting named |

A new action needs three things in one PR: the variant, the code that obeys it, and
a case in the vocabulary test. Anything else is a comment.

## Where it is applied

| Backend | Today |
|---|---|
| Samsung | with the `quirks` feature, a matching row pins the client to 8002 and 8001 leaves the ladder |
| LG | same, pinned to 3001 for firmware 23/24 |
| Roku | the 14.1 keypress gate is mapped to `Auth(NotPermitted)` in the client; the table row documents it |
| all | without the feature, each backend still negotiates its ladder and pays one refused connection |
