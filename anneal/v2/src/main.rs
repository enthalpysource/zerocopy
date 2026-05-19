// Copyright 2026 The Fuchsia Authors
//
// Licensed under the 2-Clause BSD License <LICENSE-BSD or
// https://opensource.org/license/bsd-2-clause>, Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>, or the MIT
// license <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.
// This file may not be copied, modified, or distributed except according to
// those terms.

use clap::Parser as _;

/// Anneal
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
pub struct SetupArgs {
    /// Path to a local dependency archive to use instead of downloading.
    #[arg(long, value_name = "path-to-local-archive")]
    pub local_archive: Option<std::path::PathBuf>,
}

exocrate::config! {
    const CONFIG: Config = Config {
        rel_dir_path: [".anneal", "toolchain"],
        versioned_files: &["../Cargo.toml", "../Cargo.lock"],
    };
}

exocrate::parse_remote_archive! {
    const REMOTE: RemoteArchive = "Cargo.toml" [
        (linux, x86_64),
        (macos, x86_64),
        (linux, aarch64),
        (macos, aarch64),
    ];
}

fn setup(args: SetupArgs) {
    let location = if std::env::var("__ANNEAL_LOCAL_DEV").is_ok() {
        exocrate::Location::LocalDev
    } else {
        exocrate::Location::UserGlobal
    };
    let source = match args.local_archive {
        Some(local_archive) => exocrate::Source::Local(local_archive),
        None => exocrate::Source::Remote(REMOTE),
    };

    let installation_dir = CONFIG
        .resolve_installation_dir_or_install(location, source)
        // FIXME: Implement unified error reporting (e.g., via `anyhow`).
        .expect("failed to resolve-or-install dependencies");
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
        Commands::Setup(args) => setup(args),
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
