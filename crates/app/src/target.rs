//! Packaging: a payload plus a descriptor becomes something a television can be given.
//!
//! Three targets, and the honest difference between them is what the platform's own
//! tooling still has to do:
//!
//! | target | this crate produces | still needed |
//! |---|---|---|
//! | [`Target::Tizen`] | `<id>.wgt` (a ZIP with `config.xml` at its root) | signing, for a retail TV |
//! | [`Target::WebOs`] | the app directory with `appinfo.json` | `ares-package`, for the `.ipk` |
//! | [`Target::Web`] | a plain bundle directory | nothing |
//!
//! The unpacked bundle directory is written beside the `.wgt` for every target. That
//! is not decoration: when an app fails to start on a panel, the first useful question
//! is "what exactly was in the archive", and the answer should not require unzipping.

use std::path::{Path, PathBuf};

use crate::bridge::{bridge_js_with, index_html, BridgeOptions};
use crate::bundle::Bundle;
use crate::descriptor::AppDescriptor;
use crate::error::AppError;

/// Which platform an app is being packaged for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Samsung Tizen TV: a `.wgt`.
    Tizen,
    /// LG webOS TV: an app directory for `ares-package`.
    WebOs,
    /// A plain web bundle, for a browser or a kiosk browser.
    Web,
}

impl Target {
    /// Every target.
    pub const ALL: [Target; 3] = [Target::Tizen, Target::WebOs, Target::Web];

    /// The target's name, as it is used on the command line.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tizen => "tizen",
            Self::WebOs => "webos",
            Self::Web => "web",
        }
    }
}

/// What to package, and where to put it.
#[derive(Debug, Clone)]
pub struct PackageRequest<'a> {
    /// The app's identity.
    pub descriptor: &'a AppDescriptor,
    /// Directory holding the payload — the web app to wrap.
    pub payload: &'a Path,
    /// Which platform to package for.
    pub target: Target,
    /// Directory the artifact is written into; created if missing.
    pub out_dir: &'a Path,
    /// How the bridge should reach the payload.
    pub bridge: BridgeOptions,
}

impl<'a> PackageRequest<'a> {
    /// A request with the default bridge options.
    #[must_use]
    pub fn new(
        descriptor: &'a AppDescriptor,
        payload: &'a Path,
        target: Target,
        out_dir: &'a Path,
    ) -> Self {
        Self {
            descriptor,
            payload,
            target,
            out_dir,
            bridge: BridgeOptions::default(),
        }
    }

    /// Replaces the bridge options.
    #[must_use]
    pub fn with_bridge(mut self, bridge: BridgeOptions) -> Self {
        self.bridge = bridge;
        self
    }
}

/// What packaging produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Artifact {
    /// A Tizen widget, and the unpacked bundle beside it.
    TizenWidget {
        /// The `.wgt` file.
        widget: PathBuf,
        /// The unpacked bundle directory.
        bundle: PathBuf,
    },
    /// A webOS app directory, and the command that turns it into an `.ipk`.
    WebOsAppDirectory {
        /// The app directory.
        dir: PathBuf,
        /// The `ares-package` invocation, ready to run.
        ipk_command: String,
    },
    /// A plain web bundle directory.
    WebBundle {
        /// The bundle directory.
        dir: PathBuf,
    },
}

impl Artifact {
    /// The artifact's main path: the `.wgt`, or the directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            Self::TizenWidget { widget, .. } => widget,
            Self::WebOsAppDirectory { dir, .. } => dir,
            Self::WebBundle { dir } => dir,
        }
    }

    /// The unpacked bundle directory, for every target.
    #[must_use]
    pub fn bundle(&self) -> &Path {
        match self {
            Self::TizenWidget { bundle, .. } => bundle,
            Self::WebOsAppDirectory { dir, .. } => dir,
            Self::WebBundle { dir } => dir,
        }
    }
}

/// Packages a payload for a target.
///
/// The bundle is assembled in memory — payload, then the generated bridge, then the
/// platform manifest — and written out last, so the `.wgt` and the directory beside
/// it are the same bytes by construction.
///
/// A payload that brings its own entry document keeps it: the descriptor's `entry`
/// wins over the generated fallback, and the payload's document must load
/// `tv-bridge.js` itself for the remote to work.
///
/// # Errors
///
/// - [`AppError::InvalidDescriptor`] when the descriptor does not validate.
/// - [`AppError::Io`] when the payload cannot be read or the artifact written.
pub fn package(request: &PackageRequest<'_>) -> Result<Artifact, AppError> {
    let descriptor = request.descriptor;
    descriptor.validate()?;

    let mut bundle = Bundle::from_payload_dir(request.payload)?;

    // The bridge is always ours: it is the piece that knows about remotes.
    bundle.insert("tv-bridge.js", bridge_js_with(descriptor, &request.bridge)?);

    // The entry document is the payload's if it brought one.
    if !bundle.contains(&descriptor.entry) {
        bundle.insert(descriptor.entry.clone(), index_html(descriptor)?);
    }

    match request.target {
        Target::Tizen => {
            bundle.insert("config.xml", crate::manifest::tizen_config_xml(descriptor)?)
        }
        Target::WebOs => {
            bundle.insert(
                "appinfo.json",
                crate::manifest::webos_appinfo_json(descriptor)?,
            );
        }
        Target::Web => {}
    }

    let bundle_dir = request.out_dir.join(&descriptor.id);
    bundle.write_to(&bundle_dir)?;

    match request.target {
        Target::Tizen => {
            let widget = request.out_dir.join(format!("{}.wgt", descriptor.id));
            std::fs::write(&widget, bundle.to_zip())
                .map_err(|error| AppError::io(&widget, error))?;
            Ok(Artifact::TizenWidget {
                widget,
                bundle: bundle_dir,
            })
        }
        Target::WebOs => Ok(Artifact::WebOsAppDirectory {
            ipk_command: ipk_command(&bundle_dir, request.out_dir),
            dir: bundle_dir,
        }),
        Target::Web => Ok(Artifact::WebBundle { dir: bundle_dir }),
    }
}

/// The command that turns a webOS app directory into an `.ipk`.
///
/// Returned as text rather than run: `ares-package` belongs to the webOS SDK, and a
/// packaging crate that silently shells out to a tool that may not exist is a
/// packaging crate that fails on someone else's machine.
#[must_use]
pub fn ipk_command(app_dir: &Path, out_dir: &Path) -> String {
    format!(
        "ares-package {} -o {}",
        app_dir.display(),
        out_dir.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_payload(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tv-rs-app-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("assets")).expect("mkdir");
        std::fs::write(dir.join("assets/payload.js"), b"window.tvPayload = {};").expect("write");
        dir
    }

    fn descriptor() -> AppDescriptor {
        AppDescriptor::new("com.example.wall", "Lobby wall", "1.0.0").with_vendor("Example")
    }

    #[test]
    fn a_tizen_package_is_a_zip_with_the_manifest_at_its_root() {
        let payload = fixture_payload("tizen");
        let out = std::env::temp_dir().join(format!("tv-rs-app-tizen-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);

        let descriptor = descriptor();
        let request = PackageRequest::new(&descriptor, &payload, Target::Tizen, &out);
        let artifact = package(&request).expect("package");

        let widget = match &artifact {
            Artifact::TizenWidget { widget, .. } => widget.clone(),
            other => panic!("expected a widget, got {other:?}"),
        };
        assert!(widget.exists());
        let bytes = std::fs::read(&widget).expect("read widget");
        assert_eq!(&bytes[..4], b"PK\x03\x04", "a .wgt is a ZIP");
        let archive = String::from_utf8_lossy(&bytes);
        for expected in [
            "config.xml",
            "tv-bridge.js",
            "index.html",
            "assets/payload.js",
        ] {
            assert!(archive.contains(expected), "archive should hold {expected}");
        }
        assert!(archive.contains("hwkey-event=\"enable\""));

        // The unpacked bundle is beside the widget and holds the same files.
        assert!(artifact.bundle().join("config.xml").exists());
        assert!(artifact.bundle().join("assets/payload.js").exists());

        let _ = std::fs::remove_dir_all(&payload);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn a_payload_that_brings_its_own_entry_document_keeps_it() {
        let payload = fixture_payload("own-entry");
        std::fs::write(payload.join("index.html"), b"<!DOCTYPE html><p>mine</p>").expect("write");
        let out = std::env::temp_dir().join(format!("tv-rs-app-own-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);

        let descriptor = descriptor();
        let request = PackageRequest::new(&descriptor, &payload, Target::Web, &out);
        let artifact = package(&request).expect("package");
        let entry = std::fs::read_to_string(artifact.bundle().join("index.html")).expect("read");
        assert_eq!(entry, "<!DOCTYPE html><p>mine</p>");
        assert!(artifact.bundle().join("tv-bridge.js").exists());

        let _ = std::fs::remove_dir_all(&payload);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn a_webos_package_is_a_directory_and_a_command_not_a_fake_ipk() {
        let payload = fixture_payload("webos");
        let out = std::env::temp_dir().join(format!("tv-rs-app-webos-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);

        let descriptor = descriptor();
        let request = PackageRequest::new(&descriptor, &payload, Target::WebOs, &out);
        let artifact = package(&request).expect("package");

        assert!(artifact.bundle().join("appinfo.json").exists());
        assert!(
            !out.join("com.example.wall.ipk").exists(),
            "no .ipk is fabricated"
        );
        match &artifact {
            Artifact::WebOsAppDirectory { ipk_command, .. } => {
                assert!(ipk_command.starts_with("ares-package "), "{ipk_command}");
                assert!(ipk_command.contains("com.example.wall"), "{ipk_command}");
            }
            other => panic!("expected a webOS app directory, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&payload);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn a_web_bundle_carries_no_platform_manifest() {
        let payload = fixture_payload("web");
        let out = std::env::temp_dir().join(format!("tv-rs-app-web-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);

        let descriptor = descriptor();
        let request = PackageRequest::new(&descriptor, &payload, Target::Web, &out);
        let artifact = package(&request).expect("package");
        assert!(!artifact.bundle().join("config.xml").exists());
        assert!(!artifact.bundle().join("appinfo.json").exists());
        assert!(artifact.bundle().join("index.html").exists());

        let _ = std::fs::remove_dir_all(&payload);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn packaging_is_reproducible() {
        let payload = fixture_payload("repro");
        let out_a = std::env::temp_dir().join(format!("tv-rs-app-repro-a-{}", std::process::id()));
        let out_b = std::env::temp_dir().join(format!("tv-rs-app-repro-b-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out_a);
        let _ = std::fs::remove_dir_all(&out_b);

        let descriptor = descriptor();
        let a = package(&PackageRequest::new(
            &descriptor,
            &payload,
            Target::Tizen,
            &out_a,
        ))
        .expect("package");
        let b = package(&PackageRequest::new(
            &descriptor,
            &payload,
            Target::Tizen,
            &out_b,
        ))
        .expect("package");
        assert_eq!(
            std::fs::read(a.path()).unwrap(),
            std::fs::read(b.path()).unwrap()
        );

        let _ = std::fs::remove_dir_all(&payload);
        let _ = std::fs::remove_dir_all(&out_a);
        let _ = std::fs::remove_dir_all(&out_b);
    }

    #[test]
    fn a_bad_descriptor_never_reaches_the_filesystem() {
        let payload = fixture_payload("bad-descriptor");
        let out = std::env::temp_dir().join(format!("tv-rs-app-bad-out-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let bad = AppDescriptor::new("wall", "W", "1.0.0");

        let request = PackageRequest::new(&bad, &payload, Target::Tizen, &out);
        assert!(matches!(
            package(&request),
            Err(AppError::InvalidDescriptor { field: "id", .. })
        ));
        assert!(
            !out.exists(),
            "nothing should be written for an invalid descriptor"
        );

        let _ = std::fs::remove_dir_all(&payload);
    }

    #[test]
    fn a_missing_payload_is_an_io_error_that_names_the_path() {
        let missing =
            std::env::temp_dir().join(format!("tv-rs-app-missing-{}", std::process::id()));
        let out =
            std::env::temp_dir().join(format!("tv-rs-app-missing-out-{}", std::process::id()));
        let descriptor = descriptor();
        let request = PackageRequest::new(&descriptor, &missing, Target::Web, &out);
        match package(&request) {
            Err(AppError::Io { path, .. }) => assert_eq!(path, missing),
            other => panic!("expected an io error, got {other:?}"),
        }
    }
}
