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

pub struct Toolchain {
    pub root: std::path::PathBuf,
    aeneas_bin_dir: std::path::PathBuf,
    rust_sysroot: std::path::PathBuf,
    rust_bin: std::path::PathBuf,
    rust_lib: std::path::PathBuf,
}

impl Toolchain {
    pub fn resolve() -> anyhow::Result<Self> {
        let location = if std::env::var("__ANNEAL_LOCAL_DEV").is_ok() {
            exocrate::Location::Local
        } else {
            exocrate::Location::UserGlobal
        };
        let root = CONFIG
            .resolve_installation_dir(location)
            .context("Toolchain not installed. Please run 'cargo anneal setup' first.")?;

        let aeneas_bin_dir = root.join("aeneas").join("bin");
        let rust_sysroot = root.join("rust");
        let rust_bin = rust_sysroot.join("bin");
        let rust_lib = rust_sysroot.join("lib");

        Ok(Self {
            root,
            aeneas_bin_dir,
            rust_sysroot,
            rust_bin,
            rust_lib,
        })
    }

    pub fn aeneas_bin_dir(&self) -> &std::path::Path {
        &self.aeneas_bin_dir
    }

    pub fn rust_sysroot(&self) -> &std::path::Path {
        &self.rust_sysroot
    }

    pub fn rust_bin(&self) -> &std::path::Path {
        &self.rust_bin
    }

    pub fn rust_lib(&self) -> &std::path::Path {
        &self.rust_lib
    }

    pub fn command(&self, tool: Tool) -> std::process::Command {
        std::process::Command::new(tool.path(self))
    }

    #[cfg(test)]
    pub fn new_test(root: std::path::PathBuf) -> Self {
        let aeneas_bin_dir = root.join("aeneas").join("bin");
        let rust_sysroot = root.join("rust");
        let rust_bin = rust_sysroot.join("bin");
        let rust_lib = rust_sysroot.join("lib");
        Self {
            root,
            aeneas_bin_dir,
            rust_sysroot,
            rust_bin,
            rust_lib,
        }
    }
}

pub fn run_setup(args: SetupArgs) -> anyhow::Result<()> {
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

#[cfg(feature = "exocrate_tests")]
pub fn run_test_setup() -> anyhow::Result<()> {
    // FIXME: Add GitHub actions that will block changes that would update
    // tests/toolchains/ files if TestSetup were invoked without committing them.
    
    println!("Running standard setup...");
    run_setup(SetupArgs {
        local_archive: Some("target/anneal-exocrate.tar.zst".into()),
    })?;

    let toolchain = Toolchain::resolve()?;
    let dest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("toolchains")
        .join("charon-only");

    if dest_dir.exists() {
        println!("Cleaning existing test toolchain at {:?}", dest_dir);
        std::fs::remove_dir_all(&dest_dir).context("Failed to clean test toolchain dir")?;
    }
    std::fs::create_dir_all(&dest_dir)?;

    let copy_file = |src: &std::path::Path, dest: &std::path::Path| -> anyhow::Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(src, dest)?;
        let meta = std::fs::metadata(src)?;
        std::fs::set_permissions(dest, meta.permissions())?;
        Ok(())
    };

    println!("Copying Charon binaries...");
    copy_file(
        &toolchain.aeneas_bin_dir().join("charon"),
        &dest_dir.join("aeneas").join("bin").join("charon"),
    )?;
    copy_file(
        &toolchain.aeneas_bin_dir().join("charon-driver"),
        &dest_dir.join("aeneas").join("bin").join("charon-driver"),
    )?;

    println!("Copying rustc binary...");
    copy_file(
        &toolchain.rust_bin().join("rustc"),
        &dest_dir.join("rust").join("bin").join("rustc"),
    )?;

    println!("Copying rust libraries (this may take a while)...");
    let src_lib = toolchain.rust_lib();
    let dest_lib = dest_dir.join("rust").join("lib");
    
    for entry in walkdir::WalkDir::new(&src_lib) {
        let entry = entry?;
        let rel_path = entry.path().strip_prefix(&src_lib)?;
        let dest_path = dest_lib.join(rel_path);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&dest_path)?;
        } else {
            copy_file(entry.path(), &dest_path)?;
        }
    }

    println!("Test toolchain successfully set up at {:?}", dest_dir);
    Ok(())
}

#[cfg(all(test, feature = "exocrate_tests"))]
mod tests {
    use super::*;

    #[test]
    fn test_toolchain_paths() {
        // Ensure toolchain is installed locally for test.
        unsafe { std::env::set_var("__ANNEAL_LOCAL_DEV", "1"); }
        
        let toolchain = Toolchain::resolve().expect("Failed to resolve toolchain");
        
        assert!(toolchain.root.is_dir(), "root is not a directory: {:?}", toolchain.root);
        assert!(toolchain.aeneas_bin_dir().is_dir(), "aeneas_bin_dir is not a directory: {:?}", toolchain.aeneas_bin_dir());
        assert!(toolchain.rust_sysroot().is_dir(), "rust_sysroot is not a directory: {:?}", toolchain.rust_sysroot());
        assert!(toolchain.rust_bin().is_dir(), "rust_bin is not a directory: {:?}", toolchain.rust_bin());
        assert!(toolchain.rust_lib().is_dir(), "rust_lib is not a directory: {:?}", toolchain.rust_lib());
    }
}
