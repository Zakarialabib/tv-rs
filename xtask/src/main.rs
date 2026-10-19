//! Workspace automation for `tv-rs`.
//!
//! The only command today is `publish`, which replaces the shell script that used
//! to live in `utils/scripts/`. Run it through the alias:
//!
//! ```text
//! cargo xtask publish --dry-run
//! cargo xtask publish --version 0.2.0
//! ```
//!
//! Why an xtask and not a script: the publish order is a property of the crate
//! graph (transport must reach crates.io before tizen/webos, and the facade after
//! all of them), and keeping it in the workspace means `cargo` itself can check
//! that every name in the order still exists as a package.

use std::io::Write;
use std::path::Path;
use std::process::{Command, ExitCode, Stdio};
use std::thread::sleep;
use std::time::Duration;

/// Publish order. Every path dependency must reach crates.io before its
/// dependents, so `transport` leads and the facade follows the protocol crates.
/// Keep in sync with `docs/adr/001-crate-publication.md`.
const ORDER: [&str; 8] = [
    "tv-rs-transport",
    "tv-rs-discovery",
    "tv-rs-power",
    "tv-rs-roku",
    "tv-rs-tizen",
    "tv-rs-webos",
    "tv-rs",
    "tv-rs-media",
];

/// Seconds to wait after each upload so the crates.io index catches up.
const INDEX_SETTLE: u64 = 20;

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let Some(command) = args.first() else {
        print_help();
        return Err("no command given".to_string());
    };

    match command.as_str() {
        "publish" => publish(&args[1..]),
        "-h" | "--help" | "help" => {
            print_help();
            Ok(())
        }
        other => {
            print_help();
            Err(format!("unknown command '{other}'"))
        }
    }
}

fn print_help() {
    println!(
        "workspace automation\n\n\
         USAGE:\n\
         \x20   cargo xtask publish [--dry-run] [--version X.Y.Z] [--token TOKEN] [--skip-tests] [--yes]\n\n\
         FLAGS:\n\
         \x20   --dry-run     package and verify every crate, upload nothing\n\
         \x20   --version     bump the workspace version and every pinned path dependency\n\
         \x20   --token       crates.io token (defaults to $CRATES_IO_TOKEN)\n\
         \x20   --skip-tests  skip the test and doc gates in the preflight\n\
         \x20   --yes         do not prompt before a real publish\n\n\
         ORDER:\n\
         \x20   {}",
        ORDER.join(" -> ")
    );
}

#[derive(Debug, Default)]
struct Options {
    dry_run: bool,
    version: Option<String>,
    token: Option<String>,
    skip_tests: bool,
    assume_yes: bool,
}

fn parse_options(args: &[String]) -> Result<Options, String> {
    let mut options = Options {
        token: std::env::var("CRATES_IO_TOKEN").ok(),
        ..Options::default()
    };

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--dry-run" => options.dry_run = true,
            "--skip-tests" => options.skip_tests = true,
            "--yes" | "-y" => options.assume_yes = true,
            "--version" | "--token" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| format!("{} needs a value", args[index]))?;
                if args[index] == "--version" {
                    options.version = Some(value.clone());
                } else {
                    options.token = Some(value.clone());
                }
                index += 1;
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument '{other}'")),
        }
        index += 1;
    }

    Ok(options)
}

fn publish(args: &[String]) -> Result<(), String> {
    let options = parse_options(args)?;

    if let Some(version) = options.version.as_deref() {
        bump_version(version)?;
    }

    preflight(&options)?;

    let mut publish_args: Vec<String> = Vec::new();
    if options.dry_run {
        // A dry run uploads nothing, so an uncommitted tree is expected while the
        // release metadata is being iterated on.
        publish_args.push("--dry-run".to_string());
        publish_args.push("--allow-dirty".to_string());
    } else {
        if !git_is_clean()? {
            return Err(
                "working tree is dirty — commit before publishing (a dirty release cannot be reproduced)"
                    .to_string(),
            );
        }
        let token = options
            .token
            .clone()
            .ok_or_else(|| "no crates.io token: pass --token or set CRATES_IO_TOKEN".to_string())?;
        publish_args.push("--token".to_string());
        publish_args.push(token);

        if !options.assume_yes
            && !confirm(&format!("Publish {} crates to crates.io?", ORDER.len()))?
        {
            return Err("aborted by user".to_string());
        }
    }

    for (position, package) in ORDER.iter().enumerate() {
        println!(
            "\n==> publishing {package} ({}/{})",
            position + 1,
            ORDER.len()
        );
        let mut command = Command::new("cargo");
        command
            .arg("publish")
            .arg("-p")
            .arg(package)
            .args(&publish_args);
        let output = command
            .output()
            .map_err(|error| format!("cannot run cargo: {error}"))?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        print!("{stdout}{stderr}");

        if !output.status.success() {
            // First publish: workspace path deps are not on crates.io yet, so a
            // dependent cannot resolve during a dry run. Expected, not a defect.
            if options.dry_run && stderr.contains("no matching package named `tv-rs-") {
                println!(
                    "warning: dry-run of {package} skipped — a workspace dependency is not on \
                     crates.io yet. Re-run after the earlier crates have been published once."
                );
                continue;
            }
            return Err(format!("cargo publish failed for {package}"));
        }

        if !options.dry_run {
            println!("published {package}; waiting {INDEX_SETTLE}s for the index");
            sleep(Duration::from_secs(INDEX_SETTLE));
        }
    }

    if options.dry_run {
        println!("\ndry run only — nothing was uploaded");
    }
    Ok(())
}

fn preflight(options: &Options) -> Result<(), String> {
    println!("==> preflight");
    if !Path::new("LICENSE").exists() {
        return Err("LICENSE is missing (cargo publish would reject an MIT crate)".to_string());
    }
    if !Path::new("Cargo.toml").exists() {
        return Err("Cargo.toml is missing".to_string());
    }

    gate(&["fmt", "--all", "--check"], "cargo fmt --all --check")?;
    gate(
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
        "cargo clippy --workspace --all-targets -- -D warnings",
    )?;

    if !options.skip_tests {
        gate(&["test", "--workspace"], "cargo test --workspace")?;
    }
    Ok(())
}

fn gate(args: &[&str], label: &str) -> Result<(), String> {
    println!("==> {label}");
    let status = Command::new("cargo")
        .args(args)
        .stdin(Stdio::inherit())
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{label} failed"))
    }
}

fn git_is_clean() -> Result<bool, String> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    Ok(output.stdout.is_empty())
}

fn confirm(question: &str) -> Result<bool, String> {
    print!("{question} [y/N] ");
    std::io::stdout()
        .flush()
        .map_err(|error| format!("cannot prompt: {error}"))?;
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|error| format!("cannot read the answer: {error}"))?;
    Ok(matches!(answer.trim(), "y" | "Y"))
}

/// Rewrites the workspace version and every pinned path-dependency version.
fn bump_version(version: &str) -> Result<(), String> {
    let valid = version
        .split('.')
        .take(3)
        .all(|part| part.chars().next().is_some_and(|c| c.is_ascii_digit()))
        && version.matches('.').count() >= 2;
    if !valid {
        return Err(format!("--version must look like 1.2.3 (got '{version}')"));
    }

    println!("==> bumping the workspace version to {version}");
    let path = Path::new("Cargo.toml");
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;

    let mut replaced_package = false;
    let mut replaced_deps = 0_usize;
    let mut out = String::with_capacity(text.len());

    for line in text.lines() {
        let trimmed = line.trim_start();
        if !replaced_package && trimmed.starts_with("version = \"") {
            out.push_str(&format!("version = \"{version}\""));
        } else if trimmed.starts_with("tv-rs-") && trimmed.contains("version = \"") {
            // Keep everything up to the version literal, replace the literal.
            match line.rfind("version = \"") {
                Some(at) => {
                    let head = &line[..at];
                    out.push_str(head);
                    out.push_str(&format!("version = \"{version}\""));
                    out.push_str(" }");
                    replaced_deps += 1;
                }
                None => out.push_str(line),
            }
        } else {
            out.push_str(line);
        }
        out.push('\n');
        if out.contains("version = \"") && !replaced_package {
            replaced_package = true;
        }
    }

    if replaced_deps == 0 {
        return Err("no path-dependency version pins found in Cargo.toml".to_string());
    }
    std::fs::write(path, out).map_err(|error| error.to_string())?;
    println!("version bumped ({replaced_deps} dependency pins)");
    Ok(())
}
