// Copyright 2026 The Fuchsia Authors
//
// Licensed under the 2-Clause BSD License <LICENSE-BSD or
// https://opensource.org/license/bsd-2-clause>, Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>, or the MIT
// license <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.
// This file may not be copied, modified, or distributed except according to
// those terms.

use clap::Parser as _;

mod util;
mod diagnostics;
mod resolve;
mod parse;
mod scanner;
mod charon;
mod setup;

/// Anneal: A Literate Verification Toolchain
#[derive(clap::Parser, Debug)]
#[command(name = "cargo-anneal", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand, Debug)]
enum Commands {
    /// Setup Anneal dependencies
    Setup(SetupArgs),
    /// Expand a crate (runs Charon)
    Expand(ExpandArgs),
}

#[derive(clap::Parser, Debug)]
pub struct SetupArgs {
    /// Path to a local dependency archive to use instead of downloading.
    #[arg(long, value_name = "path-to-local-archive")]
    pub local_archive: Option<std::path::PathBuf>,
}

#[derive(clap::Parser, Debug)]
pub struct ExpandArgs {
    #[command(flatten)]
    pub resolve_args: resolve::Args,

    /// Controls where LLBC output is placed on the filesystem.
    #[arg(long, value_name = "output-dir")]
    pub output_dir: Option<std::path::PathBuf>,
}

fn setup(args: SetupArgs) {
    setup::run_setup(setup::SetupArgs {
        local_archive: args.local_archive,
    })
    .expect("failed to setup toolchain");
}

fn expand(args: ExpandArgs) -> anyhow::Result<()> {
    let roots = resolve::resolve_roots(&args.resolve_args)?;
    let packages = scanner::scan_workspace(&roots)?;
    if packages.is_empty() {
        log::warn!("No targets found to expand.");
        return Ok(());
    }
    let mut locked_roots = roots.lock_run_root()?;
    if let Some(output_dir) = args.output_dir {
        locked_roots.llbc_override = Some(output_dir);
    }
    charon::run_charon(&args.resolve_args, &locked_roots, &packages)?;
    Ok(())
}

fn main() {
    // Suppressing timestamps removes a source of nondeterminism that is
    // difficult to work around in integration tests.
    env_logger::builder().format_timestamp(None).init();

    let mut args_iter = std::env::args_os().peekable();
    let bin_name = args_iter.next().unwrap_or_else(|| "cargo-anneal".into());
    // If we're being run as a cargo plugin, the second argument will be "anneal".
    if args_iter.peek().is_some_and(|arg| arg == "anneal") {
        args_iter.next();
    }
    let args = Cli::parse_from(std::iter::once(bin_name).chain(args_iter));

    match args.command {
        Commands::Setup(args) => setup(args),
        Commands::Expand(args) => {
            if let Err(e) = expand(args) {
                eprintln!("Error: {:?}", e);
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "exocrate_tests")]
    #[test]
    fn test_setup() {
        super::setup(super::SetupArgs {
            // ASSUMPTION: Dependency builder installs archive at
            // `target/anneal-exocrate.tar.zst`.
            local_archive: Some("target/anneal-exocrate.tar.zst".into()),
        })
    }
}
