// Copyright 2026 The Fuchsia Authors
//
// Licensed under the 2-Clause BSD License <LICENSE-BSD or
// https://opensource.org/license/bsd-2-clause>, Apache License, Version 2.0
// <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>, or the MIT
// license <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.
// This file may not be copied, modified, or distributed except according to
// those terms.

use std::fs;
use std::process::Command;

#[cfg(feature = "exocrate_tests")]
#[test]
fn test_expand_subcommand_simple() {
    let temp_dir = tempfile::tempdir().unwrap();
    let output_dir = temp_dir.path().join("llbc_out");

    let bin_path = std::env::var("CARGO_BIN_EXE_cargo-anneal")
        .or_else(|_| std::env::var("CARGO_BIN_EXE_cargo_anneal"))
        .expect("CARGO_BIN_EXE_* not set");

    let mut cmd = Command::new(bin_path);
    cmd.arg("expand").arg("--example").arg("simple").arg("--output-dir").arg(&output_dir);
    cmd.arg("--no-progress");

    cmd.env("__ANNEAL_LOCAL_DEV", "1");

    let output = cmd.output().expect("failed to execute cargo-anneal");

    println!("stdout: {}", String::from_utf8_lossy(&output.stdout));
    println!("stderr: {}", String::from_utf8_lossy(&output.stderr));

    assert!(output.status.success(), "cargo-anneal failed");

    let mut found_llbc = false;
    if output_dir.exists() {
        for entry in fs::read_dir(&output_dir).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_file() && path.extension().map_or(false, |ext| ext == "llbc") {
                found_llbc = true;
                break;
            }
        }
    }

    assert!(found_llbc, "No .llbc file found in output directory {:?}", output_dir);
}
