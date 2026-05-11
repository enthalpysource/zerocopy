/// Decodes a hexadecimal string into its byte representation.
const fn decode_hex(s: &str) -> Option<[u8; 32]> {
    let bytes = s.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    let mut res = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        let (h, l) = (bytes[i * 2], bytes[i * 2 + 1]);
        let h_nib = match decode_nibble(h) {
            Some(n) => n,
            None => return None,
        };
        let l_nib = match decode_nibble(l) {
            Some(n) => n,
            None => return None,
        };
        res[i] = (h_nib << 4) | l_nib;
        i += 1;
    }
    Some(res)
}

const fn decode_nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

macro_rules! decode_hex_env {
    ($key:expr) => {
        const { decode_hex(env!($key)).expect("valid hex") }
    };
}

/// Supported platforms for Anneal dependencies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    LinuxX86_64,
    LinuxAArch64,
    MacosAArch64,
    MacosX86_64,
}

impl Platform {
    /// Returns the target triple for this platform.
    pub fn triple(&self) -> &'static str {
        match self {
            Self::LinuxX86_64 => "x86_64-unknown-linux-gnu",
            Self::LinuxAArch64 => "aarch64-unknown-linux-gnu",
            Self::MacosAArch64 => "aarch64-apple-darwin",
            Self::MacosX86_64 => "x86_64-apple-darwin",
        }
    }

    /// Detects the current host platform.
    pub fn detect() -> Result<Self, String> {
        let os = std::env::consts::OS;
        let arch = std::env::consts::ARCH;

        match (os, arch) {
            ("linux", "x86_64") => Ok(Self::LinuxX86_64),
            ("linux", "aarch64") => Ok(Self::LinuxAArch64),
            ("macos", "aarch64") => Ok(Self::MacosAArch64),
            ("macos", "x86_64") => Ok(Self::MacosX86_64),
            _ => Err(format!("Unsupported platform: {}-{}", os, arch)),
        }
    }

    /// Returns the expected SHA-256 checksum for the specified archive on this
    /// platform.
    ///
    /// These hashes are baked into the binary at compile time from values
    /// provided by `build.rs` from the project's `Cargo.toml`. This ensures
    /// that we always download and verify the exact versions of dependencies
    /// that this version of Anneal was built to work with.
    pub fn expected_archive_hash(&self) -> [u8; 32] {
        use Platform::*;
        match self {
            LinuxX86_64 => decode_hex_env!("SETUP_ARCHIVE_SHA256_LINUX_X86_64"),
            LinuxAArch64 => decode_hex_env!("SETUP_ARCHIVE_SHA256_LINUX_AARCH64"),
            MacosAArch64 => decode_hex_env!("SETUP_ARCHIVE_SHA256_MACOS_AARCH64"),
            MacosX86_64 => decode_hex_env!("SETUP_ARCHIVE_SHA256_MACOS_X86_64"),
        }
    }

    pub fn remote_url(&self) -> String {
        use Platform::*;
        const BASE: &str = env!("SETUP_ARCHIVE_BASE_URL");
        match self {
            LinuxX86_64 => format!("{BASE}/anneal-toolchain-linux-x86_64.tar.zst"),
            LinuxAArch64 => format!("{BASE}/anneal-toolchain-linux-aarch64.tar.zst"),
            MacosAArch64 => format!("{BASE}/anneal-toolchain-macos-aarch64.tar.zst"),
            MacosX86_64 => format!("{BASE}/anneal-toolchain-macos-x64_64.tar.zst"),
        }
    }
}

/// Setup configuration
pub struct Config {}

pub enum Source {
    LocalDirectory(std::path::PathBuf),
    LocalTarZst(std::path::PathBuf),
    Remote,
}

fn encode_hex(bytes: &[u8; 32]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(64);
    for &b in bytes {
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}

struct HashReader<R> {
    inner: R,
    hasher: sha2::Sha256,
}

impl<R: std::io::Read> HashReader<R> {
    fn new(inner: R) -> Self {
        use sha2::Digest;
        Self {
            inner,
            hasher: sha2::Sha256::new(),
        }
    }

    fn finalize(self) -> [u8; 32] {
        use sha2::Digest;
        self.hasher.finalize().into()
    }
}

impl<R: std::io::Read> std::io::Read for HashReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        use sha2::Digest;
        let n = self.inner.read(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

fn setup_from_tar_zst(src: impl std::io::Read, dst: &std::path::Path) -> Result<[u8; 32], std::io::Error> {
    let parent = dst.parent().expect("toolchains directory has parent");
    let temp_dir = tempfile::Builder::new()
        .prefix("setup-")
        .tempdir_in(parent)?;

    let mut hash_reader = HashReader::new(src);

    let mut child = std::process::Command::new("tar")
        .arg("--zstd")
        .arg("-x")
        .arg("-C")
        .arg(temp_dir.path())
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::inherit())
        .spawn()?;

    let mut stdin = child.stdin.take().expect("Failed to open stdin");
    std::io::copy(&mut hash_reader, &mut stdin)?;
    drop(stdin); // close stdin to notify tar process of EOF

    let status = child.wait()?;
    if !status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("tar extraction failed with status: {status}"),
        ));
    }

    // Handle atomic overwrite if dst already occupies the target path
    let old_dir = if dst.symlink_metadata().is_ok() {
        let old = tempfile::Builder::new()
            .prefix("setup-old-")
            .tempdir_in(parent)?;
        let target_old = old.path().join("old");
        std::fs::rename(dst, &target_old)?;
        Some(old)
    } else {
        None
    };

    // Retain (renamed) extraction directory.
    let temp_dir_path = temp_dir.keep();
    std::fs::rename(&temp_dir_path, dst)?;

    // Drop/delete (renamed) old directory (if it exists).
    drop(old_dir);

    Ok(hash_reader.finalize())
}

pub fn setup(src: Source, toolchain_dir: std::path::PathBuf) -> Result<(), String> {
    let platform = Platform::detect().expect("detect platform");
    let expected_hash = platform.expected_archive_hash();
    let expected_hex = encode_hex(&expected_hash);
    let subdir_name = &expected_hex[..6];
    let target_dir = toolchain_dir.join(subdir_name);

    use Source::*;
    match src {
        LocalTarZst(path) => {
            log::warn!("Toolchain contents from local archive may not match expected toolchain hash/version number.");
            let file = std::fs::File::open(path).map_err(|e| format!("Failed to open local archive: {e}"))?;
            setup_from_tar_zst(file, &target_dir).map_err(|e| format!("Failed to extract archive: {e}"))?;
        }
        LocalDirectory(path) => {
            log::warn!("Toolchain contents from local directory may not match expected toolchain hash/version number.");
            #[cfg(unix)]
            {
                let old_dir = if target_dir.symlink_metadata().is_ok() {
                    let parent = target_dir.parent().unwrap();
                    let old = tempfile::Builder::new()
                        .prefix("setup-old-")
                        .tempdir_in(parent)
                        .map_err(|e| format!("Failed to create temp dir for old target: {e}"))?;
                    let target_old = old.path().join("old");
                    std::fs::rename(&target_dir, &target_old)
                        .map_err(|e| format!("Failed to rename existing target: {e}"))?;
                    Some(old)
                } else {
                    None
                };

                std::os::unix::fs::symlink(&path, &target_dir)
                    .map_err(|e| format!("Failed to symlink local directory: {e}"))?;

                drop(old_dir);
            }
            #[cfg(not(unix))]
            {
                return Err("Symlinking is only supported on Unix platforms".to_string());
            }
        }
        Remote => {
            let response = reqwest::blocking::get(&platform.remote_url())
                .map_err(|e| format!("Failed to download archive: {e}"))?;
            let response = response.error_for_status()
                .map_err(|e| format!("HTTP error downloading archive: {e}"))?;

            let actual_hash = setup_from_tar_zst(response, &target_dir)
                .map_err(|e| format!("Failed to extract downloaded archive: {e}"))?;

            if actual_hash != expected_hash {
                let _ = std::fs::remove_dir_all(&target_dir);
                return Err(format!(
                    "Checksum mismatch for downloaded archive. Expected {}, got {}",
                    expected_hex,
                    encode_hex(&actual_hash)
                ));
            }
        }
    }

    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
}
