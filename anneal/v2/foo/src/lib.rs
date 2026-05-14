// Copyright 2026 The Fuchsia Authors
//
// Licensed under the 2-Clause BSD License <LICENSE-BSD or
// https://opensource.org/license/bsd-2-clause>, Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>, or the MIT
// license <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.
// This file may not be copied, modified, or distributed except according to
// those terms.

mod sync;

use std::{
    io::{Read, Result as IoResult},
    path::{Path, PathBuf},
};

use sha2::Digest as _;

use sync::ManagedDirName;

/*
TODO:
- Document that multiple different versions of this library are guaranteed to
  be compatible *so long as* you use the executable binary-hashing approach.
- Document that any non-binary-hashing approach may result in multiple versions
  of this library being incompatible with one another, so use with caution!
  Specifically, you're still fine as long as multiple versions of a given binary
  are not live on a system at once.
*/

pub struct RemoteArchive {
    pub url: &'static str,
    /// The SHA-256 hash of the file at `url`.
    pub sha256: [u8; 32],
}

pub struct Config {
    /// The relative path of the directory containing toolchains, stored as
    /// a sequence of path components for cross-platform compatibility.
    ///
    /// In production use, toolchains are stored in `~/<rel_dir_path>/`. In
    /// development, toolchains are stored in `target/<rel_dir_path>/`.
    // FIXME: Make this non-`pub` and validate at construction that each item
    // doesn't contain any path separators.
    pub rel_dir_path: &'static [&'static str],
    /// A unique identifier for this version of the toolchain.
    pub version_slug: &'static str,
}

/// The location to install the toolchain directory.
pub enum ToolchainLocation {
    /// In production, the toolchain is stored in the user's home directory.
    Production,
    /// In development, the toolchain is stored in the Cargo `target` directory
    /// if it can be resolved, and otherwise in the current working directory.
    Development,
}

/// The source from which to install a toolchain.
pub enum ToolchainSource {
    /// Download the toolchain from the internet.
    Remote(RemoteArchive),
    /// Use a locally available toolchain.
    ///
    /// # Security
    ///
    /// When a local toolchain is provided, the hash is not validated.
    Local(PathBuf),
}

impl Config {
    /// Resolves the toolchain directory, failing if it doesn't exist.
    pub fn resolve_toolchain_dir(&self, location: ToolchainLocation) -> IoResult<PathBuf> {
        let dir_path = self.toolchain_dir_path(location);
        let _ = ManagedDirName::new(&dir_path).check_exists()?;
        Ok(dir_path)
    }

    /// Resolves the toolchain directory, installing it if needed.
    pub fn resolve_toolchain_dir_or_install(
        &self,
        location: ToolchainLocation,
        source: ToolchainSource,
    ) -> IoResult<PathBuf> {
        let dir_path = self.toolchain_dir_path(location);
        if ManagedDirName::new(&dir_path).check_exists().is_ok() {
            return Ok(dir_path);
        }
        let (reader, expected_sha) = self.open_source(source)?;
        install(reader, &dir_path, expected_sha)?;
        Ok(dir_path)
    }

    /// Opens the given source.
    ///
    /// For `ToolchainSource::Download`, returns the SHA-256 hash which should
    /// be used to validate the downloaded contents.
    fn open_source(&self, source: ToolchainSource) -> IoResult<(impl Read, Option<[u8; 32]>)> {
        enum SourceReader<D, L> {
            Download(D),
            Local(L),
        }

        impl<D: Read, L: Read> Read for SourceReader<D, L> {
            fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
                match self {
                    Self::Download(r) => r.read(buf),
                    Self::Local(r) => r.read(buf),
                }
            }
        }

        match source {
            ToolchainSource::Remote(RemoteArchive { url, sha256 }) => {
                let resp = ureq::get(url)
                    .call()
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
                let reader = resp.into_body().into_reader();
                Ok((SourceReader::Download(reader), Some(sha256)))
            }
            ToolchainSource::Local(path) => {
                let file = std::fs::File::open(path)?;
                Ok((SourceReader::Local(file), None))
            }
        }
    }

    /// Calculates the toolchain directory path:
    /// - The parent is `~` for `Production` and `CARGO_MANIFEST_DIR/target` for
    ///   `Development` (or `./target` if `CARGO_MANIFEST_DIR` is not set).
    /// - The remainder is
    ///   `.{self.dir_name}/toolchain/{self.version_slug}`.
    fn toolchain_dir_path(&self, location: ToolchainLocation) -> PathBuf {
        let mut parts = match location {
            ToolchainLocation::Production => dirs::home_dir().expect("home dir not found"),
            ToolchainLocation::Development => {
                std::env::var("CARGO_MANIFEST_DIR")
                    .map(PathBuf::from)
                    // Fall back to current directory if `CARGO_MANIFEST_DIR` is
                    // not set, which can happen if the binary is executed
                    // directly rather than via `cargo run`.
                    .unwrap_or_else(|_| std::env::current_dir().unwrap())
                    .join("target")
            }
        };

        parts.extend(self.rel_dir_path);
        parts.join(self.version_slug)
    }
}

/// Extracts the `.tar.zst` from `reader` and installs it at `dst`, optionally
/// validating its hash.
fn install(mut reader: impl Read, dst: &Path, expected_sha256: Option<[u8; 32]>) -> IoResult<()> {
    struct HashingReader<R> {
        reader: R,
        hasher: sha2::Sha256,
    }

    impl<R: std::io::Read> std::io::Read for HashingReader<R> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let n = self.reader.read(buf)?;
            sha2::Digest::update(&mut self.hasher, &buf[..n]);
            Ok(n)
        }
    }

    sync::ManagedDirName::new(dst)
        .check_exists_or_create(|target_dir| {
            if let Some(expected) = expected_sha256 {
                let mut hash_reader = HashingReader { reader, hasher: sha2::Sha256::new() };
                {
                    let decoder = zstd::stream::read::Decoder::new(&mut hash_reader)?;
                    let mut archive = tar::Archive::new(decoder);
                    archive.unpack(target_dir)?;
                }

                // Ensure any remaining trailing bytes in the stream are read
                // and hashed. Zstd may skip trailing data which isn't necessary
                // to decompress, but we need to account for it in the hash, or
                // else a valid archive could fail to hash properly if that
                // archive contains trailing data which isn't required for
                // decompression.
                std::io::copy(&mut hash_reader, &mut std::io::sink())?;

                let hash: [u8; 32] = sha2::Digest::finalize(hash_reader.hasher).into();
                if hash != expected {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "SHA-256 hash mismatch",
                    ));
                }
            } else {
                let decoder = zstd::stream::read::Decoder::new(&mut reader)?;
                let mut archive = tar::Archive::new(decoder);
                archive.unpack(target_dir)?;
            }
            Ok(())
        })
        .map(|_| ())
}

#[allow(dead_code)]
fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}

/// Initializes a [`Config`] from the given values, reading the remaining
/// values from the `Cargo.toml` at `$cargo_toml_path`.
///
/// The `Cargo.toml` is expected to have content of the form:
///
/// ```toml
/// [package.metadata.toolchain.linux.x86_64]
/// sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
/// url = "https://example.com/linux-x86_64.tar.zst"
///
/// [package.metadata.toolchain.macos.x86_64]
/// sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
/// url = "https://example.com/macos-x86_64.tar.zst"
///
/// [package.metadata.toolchain.linux.aarch64]
/// sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
/// url = "https://example.com/linux-aarch64.tar.zst"
///
/// [package.metadata.toolchain.macos.aarch64]
/// sha256 = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
/// url = "https://example.com/macos-aarch64.tar.zst"
/// ```
///
/// The `config!` invocation must specify the set of OS/Arch pairs, e.g.:
///
/// ```rust,ignore
/// config! {
///     const CONFIG: Config = Config {
///         dir_name: "my_project",
///         .. "Cargo.toml" [
///             (linux, x86_64),
///             (linux, aarch64),
///             (macos, aarch64),
///             (macos, x86_64),
///         ]
///     };
/// }
/// ```
///
/// **NOTE**: The calling crate must have its own dependency on the `toml_const`
/// crate.
// FIXME:
// - Lift this limitation.
// - Don't require the user to specify os/arch pairs in the macro invocation.
#[macro_export]
macro_rules! parse_remote_archive {
    ($vis:vis const $name:ident: RemoteArchive = $cargo_toml_path:literal [
        $(($os:ident, $arch:ident)),* $(,)?
    ];) => {
        $vis const $name: $crate::RemoteArchive = {
            ::toml_const::toml_const!{
                const MANIFEST: $cargo_toml_path;
            }

            let config = {
                use std::env::consts::*;
                use $crate::macro_util::pack;

                // NOTE: Rust doesn't support checking `&str`s for equality in a
                // `const` context. We work around that limitation by packing
                // their bytes into `u128`s, which can be compared.
                //
                // FIXME: How can we detect if os/arch pairs have been added to
                // `Cargo.toml` without being added to the macro invocation?
                match (pack(OS), pack(ARCH)) {
                    $(
                        (os, arch) if os == pack(stringify!($os)) && arch == pack(stringify!($arch)) => {
                            MANIFEST.package.metadata.toolchain.$os.$arch
                        }
                    )*
                    _ => panic!("unsupported platform"),
                }
            };

            let Some(sha256) = $crate::macro_util::decode_hex(config.sha256) else {
                panic!("invalid sha256")
            };
            $crate::RemoteArchive {
                sha256,
                url: config.url,
            }
        };
    }
}

// TODO: Better name that connotes that we're hashing everything?
#[macro_export]
macro_rules! config {
    ($vis:vis const $name:ident: Config = Config { rel_dir_path: $rel_dir_path:expr };) => {
        $vis const $name: $crate::Config = {
            $crate::Config {
                rel_dir_path: &$rel_dir_path,
                version_slug: {
                    const HEX: [u8; 64] = {
                        // FIXME: Pick a reasonably-collision-resistant hash
                        // function that is cheaper to evaluate at const time.
                        #[allow(long_running_const_eval)]
                        let hash = $crate::macro_util::Sha256::new()
                            .update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")))
                            .update(include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.lock")))
                            .update(std::env::consts::OS.as_bytes())
                            .update(std::env::consts::ARCH.as_bytes())
                            .finalize();
                        $crate::macro_util::encode_hex::<{ 32 }, { 64 }>(&hash)
                    };

                    let Ok(s) = str::from_utf8(&HEX) else {
                        unreachable!()
                    };
                    s
                }
            }
        };
    };
}

#[doc(hidden)]
pub mod macro_util {
    pub use sha2_const::Sha256;

    /// Packs the bytes of `s` into a `u128`.
    ///
    /// # Panics
    ///
    /// Panics if `s.as_bytes().len() > 16`.
    pub const fn pack(s: &str) -> u128 {
        let b = s.as_bytes();
        assert!(b.len() <= 16, "slice too large to pack into u128");

        let mut res = 0u128;
        let mut i = 0;
        while i < b.len() {
            res |= (b[i] as u128) << (i * 8);
            i += 1;
        }
        res
    }

    /// Decodes a hexadecimal string into its byte representation.
    pub const fn decode_hex(s: &str) -> Option<[u8; 32]> {
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

    pub const fn encode_hex<const N: usize, const M: usize>(bytes: &[u8; N]) -> [u8; M] {
        assert!(M == N * 2, "Output buffer must be exactly twice the input length");

        let mut res = [0u8; M];
        const HEX_TABLE: &[u8; 16] = b"0123456789abcdef";

        let mut i = 0;
        while i < N {
            res[i * 2] = HEX_TABLE[(bytes[i] >> 4) as usize];
            res[i * 2 + 1] = HEX_TABLE[(bytes[i] & 0x0f) as usize];
            i += 1;
        }
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn create_dummy_tar_zst(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut zstd_enc = zstd::stream::write::Encoder::new(Vec::new(), 0).unwrap();
        {
            let mut tar_builder = tar::Builder::new(&mut zstd_enc);
            for (name, content) in files {
                let mut header = tar::Header::new_gnu();
                header.set_size(content.len() as u64);
                header.set_mode(0o644);
                header.set_cksum();
                tar_builder.append_data(&mut header, name, *content).unwrap();
            }
            tar_builder.finish().unwrap();
        }
        zstd_enc.finish().unwrap()
    }

    fn compute_sha256(data: &[u8]) -> [u8; 32] {
        let mut hasher = sha2::Sha256::new();
        hasher.update(data);
        hasher.finalize().into()
    }

    fn dummy_config(dir_name: &'static str) -> Config {
        Config { dir_name, version_slug: "slug" }
    }

    fn dummy_remote() -> RemoteArchive {
        RemoteArchive {
            url: "https://example.com/dummy.tar.zst",
            sha256: compute_sha256(b"dummy content"),
        }
    }

    #[test]
    fn test_install_new() {
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("install_target");
        let tar_zst = create_dummy_tar_zst(&[
            ("hello.txt", b"hello world"),
            ("nested/dir/data.bin", b"\x01\x02\x03"),
        ]);
        let expected_hash = compute_sha256(&tar_zst);

        install(tar_zst.as_slice(), &dst, Some(expected_hash)).unwrap();

        assert!(dst.is_dir());
        assert_eq!(fs::read_to_string(dst.join("hello.txt")).unwrap(), "hello world");
        assert_eq!(fs::read(dst.join("nested/dir/data.bin")).unwrap(), b"\x01\x02\x03");
    }

    #[test]
    fn test_install_without_hash_validation() {
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("install_target");
        let tar_zst = create_dummy_tar_zst(&[("hello.txt", b"hello world")]);

        install(tar_zst.as_slice(), &dst, None).unwrap();

        assert!(dst.is_dir());
        assert_eq!(fs::read_to_string(dst.join("hello.txt")).unwrap(), "hello world");
    }

    #[test]
    fn test_install_invalid_archive() {
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("install_target");

        let invalid_data = b"definitely not a valid zstd or tar";

        let result = install(&invalid_data[..], &dst, None);
        assert!(result.is_err());

        assert!(!dst.exists());
        let entries: Vec<_> = fs::read_dir(temp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .filter(|n| n != "install_target.lock")
            .collect();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_install_already_exists() {
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("install_target");
        fs::create_dir_all(&dst).unwrap();
        fs::write(dst.join("existing.txt"), "existing content").unwrap();

        // Since dst already exists as a managed directory, install returns early
        // without reading the invalid archive or validating the hash.
        let bad_data = b"invalid archive data";
        let bad_hash = [0u8; 32];
        install(&bad_data[..], &dst, Some(bad_hash)).unwrap();

        assert!(dst.is_dir());
        assert_eq!(fs::read_to_string(dst.join("existing.txt")).unwrap(), "existing content");
    }

    #[test]
    fn test_install_hash_mismatch() {
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("install_target");
        let tar_zst = create_dummy_tar_zst(&[("hello.txt", b"hello world")]);

        let bad_hash = [0u8; 32];

        let result = install(tar_zst.as_slice(), &dst, Some(bad_hash));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);

        assert!(!dst.exists());
        let entries: Vec<_> = fs::read_dir(temp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .filter(|n| n != "install_target.lock")
            .collect();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_install_trailing_garbage_hash_mismatch() {
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("install_target");
        let valid_tar_zst = create_dummy_tar_zst(&[("hello.txt", b"hello world")]);
        let expected_hash = compute_sha256(&valid_tar_zst);

        // Append trailing garbage to the archive
        let mut corrupted_archive = valid_tar_zst.clone();
        corrupted_archive.extend_from_slice(b"trailing garbage data");

        // Even though the archive unpacks successfully, the trailing bytes should be
        // read and hashed, causing a hash mismatch.
        let result = install(corrupted_archive.as_slice(), &dst, Some(expected_hash));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);
    }

    #[test]
    fn test_encode_hex() {
        assert_eq!(encode_hex(b""), "");
        assert_eq!(encode_hex(b"\x00\x01\x0a\x0f\x10\xff"), "00010a0f10ff");
    }

    #[test]
    fn test_macro_util_pack() {
        assert_eq!(macro_util::pack("a"), 0x61);
        assert_eq!(macro_util::pack("ab"), 0x6261);
        assert_eq!(macro_util::pack("linux"), 0x78756e696c);
    }

    #[test]
    #[should_panic(expected = "slice too large to pack into u128")]
    fn test_macro_util_pack_too_large() {
        let _ = macro_util::pack("this_identifier_is_way_too_long");
    }

    #[test]
    fn test_macro_util_decode_hex() {
        let valid_hex = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        let decoded = macro_util::decode_hex(valid_hex).unwrap();
        assert_eq!(decoded.len(), 32);
        assert_eq!(decoded[0], 0xba);
        assert_eq!(decoded[1], 0x78);
        assert_eq!(decoded[31], 0xad);

        assert!(macro_util::decode_hex("ba78").is_none());
        let invalid_hex = "ga7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(macro_util::decode_hex(invalid_hex).is_none());
    }

    #[test]
    fn test_config_toolchain_dir_path() {
        let config = dummy_config("test_toolchain_dir_path");
        let prod_path = config.toolchain_dir_path(ToolchainLocation::Production);
        assert!(prod_path.starts_with(dirs::home_dir().unwrap()));
        assert!(prod_path.ends_with(".test_toolchain_dir_path/toolchain/slug"));

        let dev_path = config.toolchain_dir_path(ToolchainLocation::Development);
        assert!(
            dev_path.starts_with(
                std::env::var("CARGO_MANIFEST_DIR")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| std::env::current_dir().unwrap())
                    .join("target")
            )
        );
        assert!(dev_path.ends_with(".test_toolchain_dir_path/toolchain/slug"));
    }

    #[test]
    fn test_config_resolve_toolchain_dir() {
        let config = dummy_config("test_toolchain_dir_resolve");
        assert!(config.resolve_toolchain_dir(ToolchainLocation::Development).is_err());

        let dev_path = config.toolchain_dir_path(ToolchainLocation::Development);
        fs::create_dir_all(&dev_path).unwrap();

        let resolved = config.resolve_toolchain_dir(ToolchainLocation::Development).unwrap();
        assert_eq!(resolved, dev_path);

        fs::remove_dir_all(&dev_path).unwrap();
    }

    #[test]
    fn test_config_resolve_toolchain_dir_or_install_local() {
        let temp = tempfile::tempdir().unwrap();
        let archive_path = temp.path().join("archive.tar.zst");
        let dummy_tar = create_dummy_tar_zst(&[("bin/compiler", b"binary data")]);
        fs::write(&archive_path, &dummy_tar).unwrap();

        let config = Config { dir_name: "test_toolchain_dir_install", version_slug: "slug" };

        let dev_path = config.toolchain_dir_path(ToolchainLocation::Development);
        let _ = fs::remove_dir_all(&dev_path);

        let resolved = config
            .resolve_toolchain_dir_or_install(
                ToolchainLocation::Development,
                ToolchainSource::Local(archive_path),
            )
            .unwrap();
        assert_eq!(resolved, dev_path);
        assert!(dev_path.join("bin/compiler").exists());

        let resolved2 = config
            .resolve_toolchain_dir_or_install(
                ToolchainLocation::Development,
                ToolchainSource::Local(PathBuf::from("/nonexistent/path/should/not/be/accessed")),
            )
            .unwrap();
        assert_eq!(resolved2, dev_path);

        fs::remove_dir_all(&dev_path).unwrap();
    }

    #[test]
    fn test_config_open_source_not_found() {
        let config = dummy_config("test_open_source");
        let res = config
            .open_source(ToolchainSource::Local(PathBuf::from("/nonexistent/path/file.missing")));
        assert!(res.is_err());
    }

    // #[test]
    // fn test_config_macro() {
    //     config! {
    //         const CONFIG: Config = Config {
    //             dir_name: "test_project",
    //             .. "../Cargo.toml" [
    //                 (linux, x86_64),
    //                 (linux, aarch64),
    //                 (macos, x86_64),
    //                 (macos, aarch64),
    //             ]
    //         };
    //     }

    //     assert_eq!(CONFIG.dir_name, "test_project");
    //     assert_eq!(CONFIG.version_slug, "todo_slug");
    // }
}
