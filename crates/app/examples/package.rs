//! Packages a payload directory, the way a build script or another application would.
//!
//! ```text
//! cargo run -p tv-rs-app --example package -- <payload-dir> <out-dir> [tizen|webos|web] [id] [name] [version]
//! ```
//!
//! Defaults to a Tizen package of `com.example.wall` at version `1.0.0`, which is the
//! shape the tests and the docs use.

use std::path::PathBuf;
use std::process::ExitCode;

use tv_rs_app::{package, AppDescriptor, PackageRequest, Target};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let payload = match args.next() {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!(
                "usage: package <payload-dir> <out-dir> [tizen|webos|web] [id] [name] [version]"
            );
            return ExitCode::from(2);
        }
    };
    let out_dir = match args.next() {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("usage: package <payload-dir> <out-dir> [tizen|webos|web] ...");
            return ExitCode::from(2);
        }
    };

    let target = match args.next().as_deref() {
        None | Some("tizen") => Target::Tizen,
        Some("webos") => Target::WebOs,
        Some("web") => Target::Web,
        Some(other) => {
            eprintln!("unknown target {other:?}: expected tizen, webos or web");
            return ExitCode::from(2);
        }
    };

    let id = args
        .next()
        .unwrap_or_else(|| "com.example.wall".to_string());
    let name = args.next().unwrap_or_else(|| "Lobby wall".to_string());
    let version = args.next().unwrap_or_else(|| "1.0.0".to_string());
    let descriptor = AppDescriptor::new(id, name, version)
        .with_vendor("tv-rs-app example")
        .with_icon("icon.png");

    let request = PackageRequest::new(&descriptor, &payload, target, &out_dir);
    match package(&request) {
        Ok(artifact) => {
            println!(
                "packaged {} -> {}",
                target.as_str(),
                artifact.path().display()
            );
            println!("unpacked bundle: {}", artifact.bundle().display());
            if let tv_rs_app::Artifact::WebOsAppDirectory { ipk_command, .. } = &artifact {
                println!("next: {ipk_command}");
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("packaging failed: {error}");
            ExitCode::FAILURE
        }
    }
}
