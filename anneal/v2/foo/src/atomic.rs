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
    sync::{RwLock, RwLockReadGuard, RwLockWriteGuard},
};

pub struct DirLock {
    dir: PathBuf,
    // On some OSes, file locking only works reliably inter-process, not
    // intra-process. An intra-process `RwLock` fills in the gap.
    lock_file: RwLock<File>,
}

pub struct DirLockReadGuard<'a> {
    dir: &'a Path,
    guard: RwLockReadGuard<'a, File>,
}

pub struct DirLockWriteGuard<'a> {
    dir: &'a Path,
    guard: RwLockWriteGuard<'a, File>,
}

impl DirLock {
    pub fn new(dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;

        let lock_file_path = dir.join(".dirlock");
        let lock_file =
            OpenOptions::new().read(true).write(true).create(true).open(&lock_file_path)?;

        Ok(Self { dir, lock_file: RwLock::new(lock_file) })
    }

    pub fn read(&self) -> std::io::Result<DirLockReadGuard<'_>> {
        let guard = self.lock_file.read().unwrap();
        fs4::FileExt::lock_shared(&*guard)?;
        Ok(DirLockReadGuard { dir: &self.dir, guard })
    }

    pub fn write(&self) -> std::io::Result<DirLockWriteGuard<'_>> {
        let guard = self.lock_file.write().unwrap();
        <_ as fs4::FileExt>::lock(&*guard)?;
        Ok(DirLockWriteGuard { dir: &self.dir, guard })
    }

    pub fn path(&self) -> &Path {
        &self.dir
    }
}

impl<'a> DirLockReadGuard<'a> {
    pub fn path(&self) -> &Path {
        self.dir
    }
}

impl<'a> DirLockWriteGuard<'a> {
    pub fn path(&self) -> &Path {
        self.dir
    }
}

impl<'a> Drop for DirLockReadGuard<'a> {
    fn drop(&mut self) {
        if let Err(e) = self.guard.unlock() {
            log::warn!("Failed to release shared OS lock for directory: {}", e);
        }
    }
}

impl<'a> Drop for DirLockWriteGuard<'a> {
    fn drop(&mut self) {
        if let Err(e) = self.guard.unlock() {
            log::warn!("Failed to release exclusive OS lock for directory: {}", e);
        }
    }
}

/// Populates and installs a directory at `dst` atomically with respect to readers
/// utilizing `DirLock`.
///
/// This function executes the provided `populate` closure within a unique
/// temporary directory and, upon success, replaces `dst` with the newly created
/// directory.
pub(crate) fn install(
    guard: &mut DirLockWriteGuard<'_>,
    dst: &Path,
    populate: impl FnOnce(&Path) -> IoResult<()>,
) -> IoResult<()> {
    let parent = guard.path();
    assert_eq!(parent, dst.parent().expect("dst must have at least one ancestor"));
    let file_name = dst.file_name().expect("dst must have a file name");

    let tmp_dir = tempfile::Builder::new().prefix(file_name).tempdir_in(parent)?;
    populate(tmp_dir.path())?;

    if dst.exists() {
        let old_dir = tempfile::Builder::new().prefix("old_").tempdir_in(parent)?;
        let old_path = old_dir.path().to_path_buf();
        fs::remove_dir(&old_path)?;

        fs::rename(dst, &old_path)?;
        fs::rename(tmp_dir.path(), dst)?;
        let _ = fs::remove_dir_all(&old_path);
    } else {
        fs::rename(tmp_dir.path(), dst)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    #[test]
    fn test_install_success() {
        let temp = tempfile::tempdir().unwrap();
        let dirlock = DirLock::new(temp.path()).unwrap();
        let mut guard = dirlock.write().unwrap();
        let dst = temp.path().join("install_target");

        install(&mut guard, &dst, |dir| {
            fs::write(dir.join("data.txt"), "success content").unwrap();
            Ok(())
        })
        .unwrap();

        assert!(dst.is_dir());
        assert_eq!(fs::read_to_string(dst.join("data.txt")).unwrap(), "success content");
    }

    #[test]
    fn test_install_overwrite() {
        let temp = tempfile::tempdir().unwrap();
        let dirlock = DirLock::new(temp.path()).unwrap();
        let mut guard = dirlock.write().unwrap();
        let dst = temp.path().join("install_target");

        install(&mut guard, &dst, |dir| {
            fs::write(dir.join("v1.txt"), "v1").unwrap();
            Ok(())
        })
        .unwrap();

        install(&mut guard, &dst, |dir| {
            fs::write(dir.join("v2.txt"), "v2").unwrap();
            Ok(())
        })
        .unwrap();

        assert!(dst.is_dir());
        assert_eq!(fs::read_to_string(dst.join("v2.txt")).unwrap(), "v2");
        assert!(!dst.join("v1.txt").exists());
    }

    #[test]
    fn test_install_failure_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let dirlock = DirLock::new(temp.path()).unwrap();
        let mut guard = dirlock.write().unwrap();
        let dst = temp.path().join("install_target");

        let res = install(&mut guard, &dst, |dir| {
            fs::write(dir.join("partial.txt"), "partial").unwrap();
            Err(std::io::Error::new(std::io::ErrorKind::Other, "simulated error"))
        });
        assert!(res.is_err());

        assert!(!dst.exists());
        let entries: Vec<_> = fs::read_dir(temp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .filter(|n| n != ".dirlock")
            .collect();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_install_overwrite_failure_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let dirlock = DirLock::new(temp.path()).unwrap();
        let mut guard = dirlock.write().unwrap();
        let dst = temp.path().join("install_target");

        install(&mut guard, &dst, |dir| {
            fs::write(dir.join("v1.txt"), "v1").unwrap();
            Ok(())
        })
        .unwrap();

        let res = install(&mut guard, &dst, |dir| {
            fs::write(dir.join("v2.txt"), "v2").unwrap();
            Err(std::io::Error::new(std::io::ErrorKind::Other, "simulated error"))
        });
        assert!(res.is_err());

        assert!(dst.is_dir());
        assert_eq!(fs::read_to_string(dst.join("v1.txt")).unwrap(), "v1");
        assert!(!dst.join("v2.txt").exists());

        let entries: Vec<_> = fs::read_dir(temp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .filter(|n| n != ".dirlock")
            .collect();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    #[should_panic(expected = "dst must have a file name")]
    fn test_install_no_filename() {
        let temp = tempfile::tempdir().unwrap();
        let dirlock = DirLock::new(temp.path()).unwrap();
        let mut guard = dirlock.write().unwrap();
        let dst = temp.path().join("..");
        let _ = install(&mut guard, &dst, |_| Ok(()));
    }

    #[test]
    #[should_panic(expected = "dst must have at least one ancestor")]
    fn test_install_no_ancestor() {
        let temp = tempfile::tempdir().unwrap();
        let dirlock = DirLock::new(temp.path()).unwrap();
        let mut guard = dirlock.write().unwrap();
        let dst = Path::new("/");
        let _ = install(&mut guard, dst, |_| Ok(()));
    }
}
