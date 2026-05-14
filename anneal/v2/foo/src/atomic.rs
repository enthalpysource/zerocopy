// Copyright 2026 The Fuchsia Authors
//
// Licensed under the 2-Clause BSD License <LICENSE-BSD or
// https://opensource.org/license/bsd-2-clause>, Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>, or the MIT
// license <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.
// This file may not be copied, modified, or distributed except according to
// those terms.

use std::{
    fs,
    io::Result as IoResult,
    path::{Path, PathBuf},
};

/// An RAII guard that manages temporary symlink and directory paths during
/// installation.
///
/// If the installation succeeds, `disarm()` must be called to relinquish
/// ownership. Otherwise, the `Drop` implementation cleans up the temporary
/// files to prevent leaks.
struct SymlinkInstallGuard {
    link_path: PathBuf,
    dir_path: PathBuf,
    disarmed: bool,
}

impl SymlinkInstallGuard {
    fn new(link_path: PathBuf, dir_path: PathBuf) -> Self {
        Self { link_path, dir_path, disarmed: false }
    }

    fn disarm(mut self) {
        self.disarmed = true;
    }
}

impl Drop for SymlinkInstallGuard {
    fn drop(&mut self) {
        if !self.disarmed {
            let _ = fs::remove_file(&self.link_path);
            let _ = fs::remove_dir_all(&self.dir_path);
        }
    }
}

/// Populates and installs a directory at `dst` using an atomic symlink-based
/// swap.
///
/// This function executes the provided `populate` closure within a unique
/// temporary directory and, upon success, atomically updates a symlink at `dst`
/// to point to the newly created directory. Finally, it performs garbage
/// collection to clean up any previously active or orphaned directories.
///
/// # Algorithm
///
/// Standard POSIX filesystem directory replacement (`rename(2)` over a
/// non-empty directory) is non-atomic and fails with `ENOTEMPTY`. To guarantee
/// atomic updates, this implementation uses symlinks as the source of truth.
///
/// 1. Two unique temporary paths in the destination's parent directory are
///    created: one for a temporary symlink (`<tmp>`) and one for the target
///    installation directory (`<dir>-<rand>`).
/// 2. The temporary symlink `<tmp>` -> `<dir>-<rand>` is created *first*, before
///    `<dir>-<rand>` is created on disk.
///
///    If a concurrent GC scan runs at any point during directory population, it
///    will observe `<tmp>` pointing to `<dir>-<rand>` and treat `<dir>-<rand>`
///    as reachable, preventing premature deletion. This requires the symlink to
///    be created first.
/// 3. Both paths are managed by an RAII guard (`SymlinkInstallGuard`). If the
///    `populate` closure returns an error or if the process panics, the guard's
///    `Drop` automatically unlinks `<tmp>` and removes `<dir>-<rand>`.
/// 4. Once population succeeds, `rename(<tmp>, dst)` is executed. On
///    POSIX/Linux, renaming a symlink over an existing symlink is atomic. Any
///    concurrent observer reading `dst` will observe either the old directory
///    target or the new directory target. `dst` is never missing or incomplete.
/// 5. Finally, `gc` is invoked to clean up unreachable directories.
pub(crate) fn install(dst: &Path, populate: impl FnOnce(&Path) -> IoResult<()>) -> IoResult<()> {
    let parent = dst.parent().expect("dst must have at least one ancestor");
    let file_name = dst.file_name().expect("dst must have a file name");

    // 1. Generate unique paths for the target directory and the temporary symlink.
    let tmp_dir = tempfile::Builder::new().prefix(file_name).tempdir_in(parent)?;
    let dir_path = tmp_dir.path().to_path_buf();
    fs::remove_dir(&dir_path)?;
    let _ = tmp_dir.keep();

    let tmp_link = tempfile::Builder::new().prefix("tmp_link_").tempdir_in(parent)?;
    let link_path = tmp_link.path().to_path_buf();
    fs::remove_dir(&link_path)?;
    let _ = tmp_link.keep();

    let target_dir_name = dir_path.file_name().expect("dir_path must have a file name");

    // 2. Create the temporary symlink first before creating the directory.
    std::os::unix::fs::symlink(target_dir_name, &link_path)?;
    fs::create_dir(&dir_path)?;

    // 3. Guard both paths to guarantee cleanup on error.
    let guard = SymlinkInstallGuard::new(link_path, dir_path);

    populate(&guard.dir_path)?;

    // 4. Atomically swap the temporary symlink into the canonical destination.
    fs::rename(&guard.link_path, dst)?;
    guard.disarm();

    // 5. Run garbage collection to clean up any previously active or orphaned directories.
    gc(parent, dst)?;

    Ok(())
}

/// Garbage collects unreachable toolchain directories in `parent`.
fn gc(parent: &Path, canonical_dst: &Path) -> IoResult<()> {
    // On Linux, directory iteration (`fs::read_dir`) is not an atomic snapshot.
    // If a symlink is renamed while a GC process is iterating `parent`, the
    // symlink might be renamed from an unvisited position to an already-visited
    // position, causing `read_dir` to miss it. `canonical_dst` is the only
    // symlink that might be renamed, so we check it explicitly after doing our
    // initial `read_dir` to handle this race condition.
    //
    // Proof of GC Correctness:
    //
    // Let $t_{scan}$ be the window during which GC iterates `parent`. Let
    // $t_{dst}$ be the exact instant GC explicitly checks
    // `fs::read_link(canonical_dst)` strictly after iteration ($t_{scan} <
    // t_{dst}$). Let $t_{rename}$ be the instant a concurrent install renames
    // `<tmp>` to `canonical_dst`.
    //
    // - Case 1 ($t_{rename} < t_{dst}$): At $t_{dst}$, `canonical_dst` points
    //   to the new directory. Explicitly reading `canonical_dst` at $t_{dst}$
    //   captures the directory and marks it reachable.
    // - Case 2 ($t_{rename} > t_{dst}$): Since $t_{scan} < t_{dst} <
    //   t_{rename}$, `<tmp>` existed in `parent` for the entire duration of
    //   $t_{scan}$. Thus, directory iteration during $t_{scan}$ is guaranteed
    //   to yield `<tmp>`, successfully marking the directory reachable.

    use std::collections::HashSet;

    let mut directories = Vec::new();
    let mut reachable = HashSet::new();

    // Scan all entries in parent. Every directory pointed to by any symlink is
    // reachable.
    for entry_res in fs::read_dir(parent)? {
        let entry = entry_res?;
        let file_type = entry.file_type()?;

        if file_type.is_symlink() {
            if let Ok(target) = fs::read_link(entry.path()) {
                if let Some(file_name) = target.file_name() {
                    reachable.insert(file_name.to_os_string());
                }
            }
        } else if file_type.is_dir() {
            directories.push(entry.file_name());
        }
    }

    // Post-iteration verification of `canonical_dst` to close the `readdir`
    // TOCTOU gap.
    if let Ok(target) = fs::read_link(canonical_dst) {
        if let Some(file_name) = target.file_name() {
            reachable.insert(file_name.to_os_string());
        }
    }

    // Delete all unreachable directories.
    //
    // Soundness under concurrency:
    // - Concurrent GC: If multiple GC processes race to delete the same
    //   unreachable directory, one process may successfully remove files while
    //   another encounters `ENOENT` mid-deletion. Looping until
    //   `!dir_path.exists()` while ignoring errors ensures robust cleanup and
    //   clean termination once the race completes.
    // - Concurrent new installations: In-progress installations create
    //   directories using `tempfile`, which attempts directory creation in a
    //   loop checking for `EEXIST` with ~56 billion permutations (62^6). An
    //   active installation can never collide with a directory currently on
    //   disk. A previously deleted path could theoretically be reused by a
    //   new installation in between a GC worker scanning the directory's
    //   symlinks and its directories. We consider the entropy high enough to
    //   ignore this possibility, especially since it's an easily recoverable
    //   state (namely, just re-do the installation).
    for dir_name in directories {
        if !reachable.contains(&dir_name) {
            let dir_path = parent.join(dir_name);
            while dir_path.exists() {
                let _ = fs::remove_dir_all(&dir_path);
            }
        }
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
        let dst = temp.path().join("install_target");

        install(&dst, |dir| {
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
        let dst = temp.path().join("install_target");

        install(&dst, |dir| {
            fs::write(dir.join("v1.txt"), "v1").unwrap();
            Ok(())
        })
        .unwrap();

        install(&dst, |dir| {
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
        let dst = temp.path().join("install_target");

        let res = install(&dst, |dir| {
            fs::write(dir.join("partial.txt"), "partial").unwrap();
            Err(std::io::Error::new(std::io::ErrorKind::Other, "simulated error"))
        });
        assert!(res.is_err());

        assert!(!dst.exists());
        let mut entries = fs::read_dir(temp.path()).unwrap();
        assert!(entries.next().is_none());
    }

    #[test]
    fn test_install_overwrite_failure_cleanup() {
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("install_target");

        install(&dst, |dir| {
            fs::write(dir.join("v1.txt"), "v1").unwrap();
            Ok(())
        })
        .unwrap();

        let res = install(&dst, |dir| {
            fs::write(dir.join("v2.txt"), "v2").unwrap();
            Err(std::io::Error::new(std::io::ErrorKind::Other, "simulated error"))
        });
        assert!(res.is_err());

        assert!(dst.is_dir());
        assert_eq!(fs::read_to_string(dst.join("v1.txt")).unwrap(), "v1");
        assert!(!dst.join("v2.txt").exists());

        let entries = fs::read_dir(temp.path()).unwrap();
        assert_eq!(entries.count(), 2);
    }

    #[test]
    #[should_panic(expected = "dst must have a file name")]
    fn test_install_no_filename() {
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("..");
        let _ = install(&dst, |_| Ok(()));
    }

    #[test]
    #[should_panic(expected = "dst must have at least one ancestor")]
    fn test_install_no_ancestor() {
        let dst = Path::new("/");
        let _ = install(dst, |_| Ok(()));
    }

    #[test]
    fn test_gc_unreachable() {
        let temp = tempfile::tempdir().unwrap();
        let dst = temp.path().join("install_target");

        let orphan_dir = temp.path().join("orphan_dir");
        fs::create_dir(&orphan_dir).unwrap();

        install(&dst, |_| Ok(())).unwrap();

        assert!(!orphan_dir.exists());
    }
}
