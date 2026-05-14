// Copyright 2026 The Fuchsia Authors
//
// Licensed under the 2-Clause BSD License <LICENSE-BSD or
// https://opensource.org/license/bsd-2-clause>, Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>, or the MIT
// license <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.
// This file may not be copied, modified, or distributed except according to
// those terms.

mod sync;

use std::{io::Result as IoResult, path::Path};

use sha2::Digest as _;

use sync::{DirLock, DirLockReadGuard, DirLockWriteGuard};

pub struct Config {
    pub os: &'static str,
    pub arch: &'static str,
    pub url: &'static str,
    pub sha256: [u8; 32],
}

impl Config {
    // pub fn install(&self) -> IoResult<()> {}

    // pub fn install_from(&self, path: &Path) -> IoResult<()> {}

    fn toolchain_dir_name(&self) -> String {
        let sha_prefix = &self.sha256[..8];
        format!("{}-{}-{}", self.os, self.arch, encode_hex(sha_prefix))
    }
}

/// Extracts the `.tar.zst` from `reader` and installs it at `dst`, optionally
/// validating its hash.
pub(crate) fn install(
    guard: &mut DirLockWriteGuard<'_>,
    mut reader: impl std::io::Read,
    dst: &Path,
    expected_sha256: Option<[u8; 32]>,
) -> IoResult<()> {
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

    sync::install(guard, dst, |target_dir| {
        if let Some(expected) = expected_sha256 {
            let mut hash_reader = HashingReader { reader, hasher: sha2::Sha256::new() };
            {
                let decoder = zstd::stream::read::Decoder::new(&mut hash_reader)?;
                let mut archive = tar::Archive::new(decoder);
                archive.unpack(target_dir)?;
            }

            let hash: [u8; 32] = sha2::Digest::finalize(hash_reader.hasher).into();
            if hash != expected {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "sha256 hash mismatch",
                ));
            }
        } else {
            let decoder = zstd::stream::read::Decoder::new(&mut reader)?;
            let mut archive = tar::Archive::new(decoder);
            archive.unpack(target_dir)?;
        }
        Ok(())
    })
}

fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        write!(&mut s, "{:02x}", b).unwrap();
    }
    s
}

/// **NOTE**: The calling crate must have its own dependency on the `toml_const`
/// crate.
// FIXME:
// - Lift this limitation.
// - Don't require the user to specify os/arch pairs in the macro invocation.
#[macro_export]
macro_rules! config {
    ($vis:vis const $name:ident: Config = $cargo_toml_path:literal [
        $(($os:ident, $arch:ident)),* $(,)?
    ];) => {
        $vis const $name: $crate::Config = {
            ::toml_const::toml_const!{
                const MANIFEST: $cargo_toml_path;
            }

            let (os, arch, config) = {
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
                            let (os, arch) = (stringify!($os), stringify!($arch));
                            let config = MANIFEST.package.metadata.toolchain.$os.$arch;
                            (os, arch, config)
                        }
                    )*
                    _ => panic!("unsupported platform"),
                }
            };

            let Some(sha256) = $crate::macro_util::decode_hex(config.sha256) else {
                panic!("invalid sha256")
            };
            $crate::Config {
                os,
                arch,
                url: config.url,
                sha256,
            }
        };
    }
}

#[doc(hidden)]
pub mod macro_util {
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

    #[test]
    fn test_install_new() {
        let temp = tempfile::tempdir().unwrap();
        let dirlock = DirLock::new(temp.path()).unwrap();
        let mut guard = dirlock.write().unwrap();
        let dst = temp.path().join("install_target");
        let tar_zst = create_dummy_tar_zst(&[
            ("hello.txt", b"hello world"),
            ("nested/dir/data.bin", b"\x01\x02\x03"),
        ]);
        let expected_hash = compute_sha256(&tar_zst);

        install(&mut guard, tar_zst.as_slice(), &dst, Some(expected_hash)).unwrap();

        assert!(dst.is_dir());
        assert_eq!(fs::read_to_string(dst.join("hello.txt")).unwrap(), "hello world");
        assert_eq!(fs::read(dst.join("nested/dir/data.bin")).unwrap(), b"\x01\x02\x03");
    }

    #[test]
    fn test_install_overwrite_dir() {
        let temp = tempfile::tempdir().unwrap();
        let dirlock = DirLock::new(temp.path()).unwrap();
        let mut guard = dirlock.write().unwrap();
        let dst = temp.path().join("install_target");

        let tar_v1 = create_dummy_tar_zst(&[("old_file.txt", b"this should be deleted")]);
        install(&mut guard, tar_v1.as_slice(), &dst, None).unwrap();
        assert_eq!(fs::read_to_string(dst.join("old_file.txt")).unwrap(), "this should be deleted");

        let tar_v2 = create_dummy_tar_zst(&[("new_file.txt", b"new content")]);
        let expected_hash = compute_sha256(&tar_v2);
        install(&mut guard, tar_v2.as_slice(), &dst, Some(expected_hash)).unwrap();

        assert!(dst.is_dir());
        assert_eq!(fs::read_to_string(dst.join("new_file.txt")).unwrap(), "new content");
        assert!(!dst.join("old_file.txt").exists());
    }

    #[test]
    fn test_install_without_hash_validation() {
        let temp = tempfile::tempdir().unwrap();
        let dirlock = DirLock::new(temp.path()).unwrap();
        let mut guard = dirlock.write().unwrap();
        let dst = temp.path().join("install_target");
        let tar_zst = create_dummy_tar_zst(&[("hello.txt", b"hello world")]);

        install(&mut guard, tar_zst.as_slice(), &dst, None).unwrap();

        assert!(dst.is_dir());
        assert_eq!(fs::read_to_string(dst.join("hello.txt")).unwrap(), "hello world");
    }

    #[test]
    fn test_install_invalid_archive() {
        let temp = tempfile::tempdir().unwrap();
        let dirlock = DirLock::new(temp.path()).unwrap();
        let mut guard = dirlock.write().unwrap();
        let dst = temp.path().join("install_target");

        let invalid_data = b"definitely not a valid zstd or tar";

        let result = install(&mut guard, &invalid_data[..], &dst, None);
        assert!(result.is_err());

        assert!(!dst.exists());
    }

    #[test]
    fn test_install_hash_mismatch() {
        let temp = tempfile::tempdir().unwrap();
        let dirlock = DirLock::new(temp.path()).unwrap();
        let mut guard = dirlock.write().unwrap();
        let dst = temp.path().join("install_target");
        let tar_zst = create_dummy_tar_zst(&[("hello.txt", b"hello world")]);

        let bad_hash = [0u8; 32];

        let result = install(&mut guard, tar_zst.as_slice(), &dst, Some(bad_hash));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);

        assert!(!dst.exists());
        let entries: Vec<_> = fs::read_dir(temp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .filter(|n| n != ".dirlock")
            .collect();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_install_overwrite_dir_hash_mismatch() {
        let temp = tempfile::tempdir().unwrap();
        let dirlock = DirLock::new(temp.path()).unwrap();
        let mut guard = dirlock.write().unwrap();
        let dst = temp.path().join("install_target");

        let tar_v1 = create_dummy_tar_zst(&[("existing.txt", b"do not touch")]);
        install(&mut guard, tar_v1.as_slice(), &dst, None).unwrap();

        let tar_v2 = create_dummy_tar_zst(&[("new_file.txt", b"new content")]);
        let bad_hash = [1u8; 32];

        let result = install(&mut guard, tar_v2.as_slice(), &dst, Some(bad_hash));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidData);

        assert!(dst.is_dir());
        assert_eq!(fs::read_to_string(dst.join("existing.txt")).unwrap(), "do not touch");
        assert!(!dst.join("new_file.txt").exists());

        let entries: Vec<_> = fs::read_dir(temp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .filter(|n| n != ".dirlock")
            .collect();
        assert_eq!(entries.len(), 1);
    }
}
