//! Thin binary entry point for the `tv-rs` CLI.
//!
//! All command definitions, execution and rendering live in the library half of
//! this package — see the [`tv_rs_cli`] crate root. This file only parses the
//! arguments and maps the outcome to a process [`ExitCode`].

use std::process::ExitCode;

use clap::Parser;

use tv_rs_cli::cli::Cli;

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    match tv_rs_cli::commands::execute(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}
