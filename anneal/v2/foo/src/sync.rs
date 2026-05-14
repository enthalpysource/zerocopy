// Copyright 2026 The Fuchsia Authors
//
// Licensed under the 2-Clause BSD License <LICENSE-BSD or
// https://opensource.org/license/bsd-2-clause>, Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>, or the MIT
// license <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.
// This file may not be copied, modified, or distributed except according to
// those terms.

use std::{
    fs::{self, File, OpenOptions},
    io::Result as IoResult,
    path::{Path, PathBuf},
};

/// A directory managed by this library.
///
/// A `ManagedDir` has the following properties:
/// - The directory itself is guaranteed to be atomic. That is, if a directory
///   at the path exists, then it has already been fully populated. This means
///   that no locking is required to check whether the directory exists.
/// - Populating and installing the directory requires a lock. In other words,
///   writers are required to actively synchronize with one another. This
///   simplifies the writer implementation by avoiding the necessity for
///   complex wait-free synchronization logic.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ManagedDir<'a> {
    path: &'a Path,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ManagedDirName<'a> {
    path: &'a Path,
}

impl<'a> ManagedDirName<'a> {
    pub const fn new(path: &'a Path) -> Self {
        Self { path }
    }

    /// Checks if the directory exists.
    pub fn check_exists(self) -> IoResult<ManagedDir<'a>> {
        if self.path.try_exists()? {
            Ok(ManagedDir { path: self.path })
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Managed directory does not exist",
            ))
        }
    }

    /// Checks if the directory exists, creating it if not.
    ///
    /// # Concurrency
    ///
    /// `check_exists_or_create` is **not** concurrency-safe if multiple
    /// concurrent calls are made *from the same process*.
    pub fn check_exists_or_create(
        self,
        populate: impl FnOnce(&Path) -> IoResult<()>,
    ) -> IoResult<ManagedDir<'a>> {
        if let Some(parent) = self.path.parent() {
            // NOTE: `create_dir_all` is safe to call concurrently with other
            // processes attempting to create the same directory:
            //
            //  Notable exception [to the error conditions] is made for
            //  situations where any of the directories specified in the path
            //  could not be created as it was being created concurrently. Such
            //  cases are considered to be successful. That is, calling
            //  create_dir_all concurrently from multiple threads or processes
            //  is guaranteed not to fail due to a race condition with itself.
            fs::create_dir_all(parent)?;
        }

        if let Ok(dir) = self.check_exists() {
            return Ok(dir);
        }

        let lock_file_path = self.lock_path();
        let lock_file =
            OpenOptions::new().read(true).write(true).create(true).open(&lock_file_path)?;

        <_ as fs4::FileExt>::lock(&lock_file)?;

        let staging_dir = self.staging();

        struct StagingGuard<'a> {
            staging_dir: &'a Path,
            lock_file: File,
            completed: bool,
        }

        impl Drop for StagingGuard<'_> {
            fn drop(&mut self) {
                if !self.completed {
                    let _ = fs::remove_dir_all(&self.staging_dir);
                }
                let _ = <_ as fs4::FileExt>::unlock(&self.lock_file);
            }
        }

        // A guard that will clean up even if `populate` panics.
        let mut guard = StagingGuard { staging_dir: &staging_dir, lock_file, completed: false };

        // Handle the case where, while we were waiting to acquire the lock,
        // another process populated the directory.
        if let Ok(dir) = self.check_exists() {
            guard.completed = true;
            return Ok(dir);
        }

        // Clean up from any previous failed attempt to populate the directory.
        let _ = fs::remove_dir_all(&guard.staging_dir);
        fs::create_dir_all(&guard.staging_dir)?;

        populate(&guard.staging_dir)?;

        fs::rename(&guard.staging_dir, self.path).map_err(|e| {
            std::io::Error::new(
                e.kind(),
                format!(
                    "Failed to rename staging directory to target path (this indicates a {}): {}",
                    if cfg!(windows) {
                        "manual modification, concurrency bug, or open file handle"
                    } else {
                        "manual modification or concurrency bug"
                    },
                    e
                ),
            )
        })?;

        guard.completed = true;
        Ok(ManagedDir { path: self.path })
    }

    fn staging(&self) -> PathBuf {
        let mut file_name =
            self.path.file_name().expect("ManagedDirName path must have a filename").to_os_string();
        file_name.push(".staging");
        self.path.with_file_name(file_name)
    }

    fn lock_path(&self) -> PathBuf {
        let mut file_name =
            self.path.file_name().expect("ManagedDirName path must have a filename").to_os_string();
        file_name.push(".lock");
        self.path.with_file_name(file_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_check_exists_or_create_success() {
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("install_target");
        let managed = ManagedDirName::new(&dst);

        let dir = managed
            .check_exists_or_create(|staging| {
                fs::write(staging.join("data.txt"), "success content").unwrap();
                Ok(())
            })
            .unwrap();

        assert_eq!(dir.path, dst.as_path());
        assert!(dst.is_dir());
        assert_eq!(fs::read_to_string(dst.join("data.txt")).unwrap(), "success content");
    }

    #[test]
    fn test_check_exists_or_create_already_exists() {
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("install_target");
        let managed = ManagedDirName::new(&dst);

        managed
            .check_exists_or_create(|staging| {
                fs::write(staging.join("v1.txt"), "v1").unwrap();
                Ok(())
            })
            .unwrap();

        let dir = managed
            .check_exists_or_create(|_| {
                panic!("should not be called on already existing directory");
            })
            .unwrap();

        assert_eq!(dir.path, dst.as_path());
        assert!(dst.is_dir());
        assert_eq!(fs::read_to_string(dst.join("v1.txt")).unwrap(), "v1");
        assert!(!dst.join("v2.txt").exists());
    }

    #[test]
    fn test_check_exists_or_create_failure_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("install_target");
        let managed = ManagedDirName::new(&dst);

        let res = managed.check_exists_or_create(|staging| {
            fs::write(staging.join("partial.txt"), "partial").unwrap();
            Err(std::io::Error::new(std::io::ErrorKind::Other, "simulated error"))
        });
        assert!(res.is_err());

        assert!(!dst.exists());
        let entries: Vec<_> = fs::read_dir(temp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .filter(|n| n != "install_target.lock")
            .collect();
        assert!(entries.is_empty());
    }

    #[test]
    #[should_panic(expected = "ManagedDirName path must have a filename")]
    fn test_no_filename() {
        let dst = Path::new("");
        let managed = ManagedDirName::new(dst);
        let _ = managed.check_exists_or_create(|_| Ok(()));
    }

    #[test]
    fn test_managed_dir() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("managed");
        let managed_name = ManagedDirName::new(&target);

        assert!(managed_name.check_exists().is_err());

        let dir = managed_name
            .check_exists_or_create(|staging| {
                fs::write(staging.join("test.txt"), "hello").unwrap();
                Ok(())
            })
            .unwrap();

        assert_eq!(dir.path, target.as_path());
        assert!(target.join("test.txt").exists());

        // Second call should return early via check_exists
        let dir2 = managed_name
            .check_exists_or_create(|_| {
                panic!("should not be called");
            })
            .unwrap();

        assert_eq!(dir2.path, target.as_path());
    }
}
