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

pub struct Toolchain {
    root: std::path::PathBuf,
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
        Ok(Self { root })
    }

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

    #[cfg(test)]
    pub fn new_test(root: std::path::PathBuf) -> Self {
        Self { root }
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
    log::debug!("Running standard setup...");
    run_setup(SetupArgs {
        local_archive: Some("target/anneal-exocrate.tar.zst".into()),
    })?;

    let toolchain = Toolchain::resolve()?;
    let dest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("toolchains")
        .join("charon-only");

    if dest_dir.exists() {
        log::debug!("Cleaning existing test toolchain at {:?}", dest_dir);
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

    log::debug!("Copying Charon binaries...");
    copy_file(
        &toolchain.aeneas_bin_dir().join("charon"),
        &dest_dir.join("aeneas").join("bin").join("charon"),
    )?;
    copy_file(
        &toolchain.aeneas_bin_dir().join("charon-driver"),
        &dest_dir.join("aeneas").join("bin").join("charon-driver"),
    )?;

    log::debug!("Copying rustc binary...");
    copy_file(
        &toolchain.rust_bin().join("rustc"),
        &dest_dir.join("rust").join("bin").join("rustc"),
    )?;

    log::debug!("Copying rust libraries (this may take a while)...");
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

    log::debug!("Test toolchain successfully set up at {:?}", dest_dir);
    Ok(())
}


