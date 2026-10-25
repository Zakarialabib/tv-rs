# tv-rs-app

Package a supplied web/wasm payload as an installable TV app.

A television app is a web app in a wrapper: a directory of files plus a manifest the
platform reads to decide whether to install it, what to call it in the app list, and —
on Tizen — whether to deliver remote-control key events to it at all. This crate writes
that wrapper around a payload you already have.

**What it is not.** It does not play content, schedule anything or talk to a CMS. The
payload is yours; this crate's job ends when the platform's tooling can take over.

```toml
[dependencies]
tv-rs-app = "0.3"
```

```rust
use std::path::Path;
use tv_rs_app::{package, AppDescriptor, PackageRequest, Target};

let descriptor = AppDescriptor::new("com.example.wall", "Lobby wall", "1.0.0")
    .with_vendor("Example Retail")
    .with_icon("icon.png");

let artifact = package(&PackageRequest::new(
    &descriptor,
    Path::new("payload"),
    Target::Tizen,
    Path::new("dist"),
))?;
println!("{}", artifact.path().display());
```

## What comes out

| target | this crate produces | still needed |
|---|---|---|
| `Target::Tizen` | `<id>.wgt` — a ZIP with `config.xml` at its root | signing, for a retail TV |
| `Target::WebOs` | the app directory with `appinfo.json` | `ares-package`, for the `.ipk` |
| `Target::Web` | a plain bundle directory | nothing |

The unpacked bundle directory is written beside the `.wgt` for every target. When an
app fails to start on a panel, the first useful question is "what exactly was in the
archive", and answering it should not require unzipping.

`tv-rs-app` does not fabricate an `.ipk`. A webOS `.ipk` is an `ar` archive holding
`control.tar.gz` and `data.tar.gz` with a platform control file; `package()` returns
the app directory and `ipk_command()` returns the `ares-package` invocation instead.
Signing and installing are out of scope on purpose — those belong to the operator and
the panel.

## The bridge

A payload is a web app that knows nothing about televisions, so the crate generates the
piece that does: `tv-bridge.js`. It maps DOM key events through one small vocabulary
and calls the payload's `key_event(name, value)`. Every call is optional — a payload
that implements only `key_event` is a complete payload.

| the bridge calls | when |
|---|---|
| `boot()` | once, when the document is ready |
| `tick(now_ms)` | every animation frame |
| `key_event(name, value)` | on a remote key; `value` is `1` held, `0` released |
| `dispatch(json)` | when the app calls `window.tvBridge.send(json)` |
| `snapshot()` | when the app calls `window.tvBridge.snapshot()` |

Key names are `Up`, `Down`, `Left`, `Right`, `Select`, `Back`, `Home`, `Exit`, `Play`,
`Pause`, `Stop`.

**Back and exit are the platform-specific ones.** Tizen and webOS deliver different
event names for back (`GoBack` and `BrowserBack`) — both are accepted — and different
*numbers* for back and exit, which have also moved between model years. The numbers are
a deployment fact, so they are supplied through `BridgeOptions::with_key_code` rather
than shipped as a guess: a wrong guess is an app whose back button silently does
nothing on half the walls.

If your payload brings its own entry document, it is kept as-is, and it must load
`tv-bridge.js` itself for the remote to work.

## The one field whose absence is silent

`config.xml` carries `tizen:setting hwkey-event="enable"`. Without it a Tizen app runs
perfectly and simply never receives a key press. It is set for you, and there is a test
asserting it stays set.

## Trying it

```text
cargo run -p tv-rs-app --example package -- <payload-dir> <out-dir> [tizen|webos|web] [id] [name] [version]
```

The artifacts are byte-identical for the same input: entries are stored, not deflated,
and timestamps are fixed at the ZIP epoch. A packaged artifact you can diff is an
artifact you can review.

## Where the protocol facts come from

The manifest fields and key names are documented in the workspace's
[`docs/02-playbook.md`](../../docs/02-playbook.md) §3 (Tizen) and §4 (webOS), and each
one is explained where it is set in `manifest.rs` and `keys.rs`.

## Version note

`tv-rs-app` is new, and publishes with the rest of the workspace at `0.3.0`. Check what the registry actually
serves with `cargo search tv-rs-app` — this workspace keeps install snippets honest
rather than optimistic.
