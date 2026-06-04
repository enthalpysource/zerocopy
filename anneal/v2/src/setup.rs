// Copyright 2026 The Fuchsia Authors
//
// Licensed under the 2-Clause BSD License <LICENSE-BSD or
// https://opensource.org/license/bsd-2-clause>, Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>, or the MIT
// license <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.
// This file may not be copied, modified, or distributed except according to
// those terms.

use anyhow::Context as _;

pub struct SetupArgs {
    pub local_archive: Option<std::path::PathBuf>,
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

    pub fn path(&self, toolchain: &Toolchain) -> std::path::PathBuf {
        match self {
            Self::Charon => toolchain.aeneas_bin_dir().join(self.name()),
        }
    }
}

const AENEAS_DIR: &str = "aeneas";
const RUST_SYSROOT: &str = "rust";
const AENEAS_BIN_DIR: &str = "bin";
const RUST_BIN_DIR: &str = "bin";
const RUST_LIB_DIR: &str = "lib";
const PLACEHOLDER_ROOT: &str = "/ANNEAL_PLACEHOLDER_ROOT";

pub struct Toolchain {
    root: std::path::PathBuf,
}

impl Toolchain {
    pub fn resolve() -> anyhow::Result<Self> {
        let location = if std::env::var("__ANNEAL_LOCAL_DEV").is_ok() {
            exocrate::Location::LocalDev
        } else {
            exocrate::Location::UserGlobal
        };
        let root = CONFIG
            .resolve_installation_dir(location)
            .context("Toolchain not installed. Please run 'cargo anneal setup' first.")?;
        Ok(Self { root })
    }

    #[allow(dead_code)]
    pub fn root(&self) -> &std::path::Path {
        &self.root
    }

    pub fn aeneas_bin_dir(&self) -> std::path::PathBuf {
        self.root.join(AENEAS_DIR).join(AENEAS_BIN_DIR)
    }

    pub fn rust_sysroot(&self) -> std::path::PathBuf {
        self.root.join(RUST_SYSROOT)
    }

    pub fn rust_bin(&self) -> std::path::PathBuf {
        self.rust_sysroot().join(RUST_BIN_DIR)
    }

    pub fn rust_lib(&self) -> std::path::PathBuf {
        self.rust_sysroot().join(RUST_LIB_DIR)
    }

    pub fn command(&self, tool: Tool) -> std::process::Command {
        std::process::Command::new(tool.path(self))
    }
}

pub fn run_setup(args: SetupArgs) -> anyhow::Result<()> {
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
        .resolve_installation_dir_or_install_with_fixup(location, source, fixup_toolchain_install)
        .context("failed to resolve-or-install dependencies")?;
    log::info!("anneal toolchain is installed at {:?}", installation_dir);
    Ok(())
}

fn fixup_toolchain_install(root: &std::path::Path) -> std::io::Result<()> {
    set_readonly_recursive(root, false)?;
    rewrite_trace_placeholders(root)?;
    set_readonly_recursive(root, true)
}

fn rewrite_trace_placeholders(root: &std::path::Path) -> std::io::Result<()> {
    let replacement = root.to_string_lossy();
    for entry in walkdir::WalkDir::new(root) {
        let entry = entry.map_err(std::io::Error::other)?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "trace") {
            let content = std::fs::read_to_string(path)?;
            if content.contains(PLACEHOLDER_ROOT) {
                std::fs::write(path, content.replace(PLACEHOLDER_ROOT, replacement.as_ref()))?;
            }
        }
    }
    Ok(())
}

fn set_readonly_recursive(root: &std::path::Path, readonly: bool) -> std::io::Result<()> {
    let mut paths = walkdir::WalkDir::new(root)
        .into_iter()
        .map(|entry| entry.map(|entry| entry.into_path()).map_err(std::io::Error::other))
        .collect::<std::io::Result<Vec<_>>>()?;

    if readonly {
        paths.reverse();
    }

    for path in paths {
        let mut perms = std::fs::metadata(&path)?.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(readonly);
        std::fs::set_permissions(path, perms)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixup_rewrites_trace_placeholders() {
        let temp = tempfile::tempdir().unwrap();
        let trace_dir = temp.path().join("aeneas/backends/lean/.lake/build/lib/lean");
        std::fs::create_dir_all(&trace_dir).unwrap();
        let trace = trace_dir.join("Aeneas.trace");
        std::fs::write(&trace, format!("{PLACEHOLDER_ROOT}/aeneas/backends/lean/Aeneas.lean"))
            .unwrap();

        fixup_toolchain_install(temp.path()).unwrap();

        let content = std::fs::read_to_string(&trace).unwrap();
        assert!(content.contains(temp.path().to_string_lossy().as_ref()));
        assert!(!content.contains(PLACEHOLDER_ROOT));
        assert!(std::fs::metadata(&trace).unwrap().permissions().readonly());
    }
}
