// Copyright 2026 The Fuchsia Authors
//
// Licensed under the 2-Clause BSD License <LICENSE-BSD or
// https://opensource.org/license/bsd-2-clause>, Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>, or the MIT
// license <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.
// This file may not be copied, modified, or distributed except according to
// those terms.

use clap::Parser as _;

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
}
#[derive(clap::Parser, Debug)]
pub struct SetupArgs {}

exocrate::config! {
    const CONFIG: Config = Config { rel_dir_path: [".anneal", "toolchain"] };
}

exocrate::parse_remote_archive! {
    const REMOTE: RemoteArchive = "Cargo.toml" [
        (linux, x86_64),
        (macos, x86_64),
        (linux, aarch64),
        (macos, aarch64),
    ];
}

fn exocrate_location_and_source() -> (exocrate::Location, exocrate::Source) {
    if std::env::var("__ANNEAL_LOCAL_DEV").is_ok() {
        (
            exocrate::Location::Local,
            exocrate::Source::Local(
                std::env::var("CARGO_MANIFEST_DIR")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|_| std::env::current_dir().unwrap())
                    // ASSUMPTION: Output target directory is `target/`.
                    .join("target")
                    // ASSUMPTION: Toolchain builder targets `<output-directory>/anneal-exocrate.tar.zst`.
                    .join("anneal-exocrate.tar.zst"),
            ),
        )
    } else {
        (exocrate::Location::UserGlobal, exocrate::Source::Remote(REMOTE))
    }
}

fn setup() {
    let (location, source) = exocrate_location_and_source();
    let installation_dir = CONFIG
        .resolve_installation_dir_or_install(location, source)
        .expect("failed to resolve-or-install exocrate");
    log::info!("anneal toolchain is installed at {:?}", installation_dir);
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
        Commands::Setup(_) => setup(),
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "exocrate_tests")]
    mod exocrate {
        use super::super::*;
        #[test]
        fn test_setup() {
            setup()
        }
    }
}
