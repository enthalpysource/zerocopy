use std::path::PathBuf;
use std::process::Command;
use anyhow::{Result, Context};

pub struct SetupArgs {
    pub local_archive: Option<PathBuf>,
}

exocrate::config! {
    pub const CONFIG: Config = Config {
        rel_dir_path: [".anneal", "toolchain"],
        versioned_files: &["../Cargo.toml", "../Cargo.lock"],
    };
}

exocrate::parse_remote_archive! {
    pub const REMOTE: RemoteArchive = "Cargo.toml" [
        (linux, x86_64),
        (macos, x86_64),
        (linux, aarch64),
        (macos, aarch64),
    ];
}

pub enum Tool {
    Charon,
}

impl Tool {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Charon => "charon",
        }
    }
}

pub struct Toolchain {
    pub root: PathBuf,
}

impl Toolchain {
    pub fn resolve() -> Result<Self> {
        let location = if std::env::var("__ANNEAL_LOCAL_DEV").is_ok() {
            exocrate::Location::Local
        } else {
            exocrate::Location::UserGlobal
        };
        let root = CONFIG
            .resolve_installation_dir(location)
            .context("Toolchain not installed. Please run 'cargo anneal setup' first.")?;
        Ok(Self { root })
    }

    pub fn command(&self, tool: Tool) -> Command {
        let bin_name = tool.name();
        let managed_path = self.root.join("aeneas").join("bin").join(bin_name);
        Command::new(managed_path)
    }
}

pub fn run_setup(args: SetupArgs) -> Result<()> {
    let location = if std::env::var("__ANNEAL_LOCAL_DEV").is_ok() {
        exocrate::Location::Local
    } else {
        exocrate::Location::UserGlobal
    };
    let source = match args.local_archive {
        Some(local_archive) => exocrate::Source::Local(local_archive),
        None => exocrate::Source::Remote(REMOTE),
    };

    let installation_dir = CONFIG
        .resolve_installation_dir_or_install(location, source)
        .context("failed to resolve-or-install dependencies")?;
    log::info!("anneal toolchain is installed at {:?}", installation_dir);
    Ok(())
}
