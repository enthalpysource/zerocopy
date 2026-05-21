//! Orchestration of Charon extraction.
//!
//! This module handles the invocation of the `charon` tool to extract
//! Low-Level Borrow Calculus (LLBC) from Rust crates. It manages:
//! - Setting up the Charon command and arguments (including features,
//!   targets, and output paths).
//! - Handling `unsafe(axiom)` functions by marking them as opaque to Charon.
//! - Streaming and filtering compiler output to provide user-friendly
//!   feedback via `indicatif` and `miette`.
//! - Validating the extraction result.

use anyhow::Context as _;
use rayon::prelude::IntoParallelRefIterator as _;
use rayon::prelude::ParallelIterator as _;

/// Runs Charon on the specified packages to generate LLBC artifacts.
///
/// This function requires [`crate::resolve::LockedRoots`] to ensure that it has exclusive access
/// to the `llbc` output directory. It iterates over each [`crate::scanner::AnnealArtifact`],
/// constructs the appropriate `charon` command, and executes it.
///
/// It handles:
/// - **Opaque Functions**: identifying `unsafe(axiom)` functions and passing
///   `--opaque` to Charon.
/// - **Entry Points**: passing the computed `start_from` roots to Charon to
///   minimize extraction scope.
/// - **Output Handling**: capturing stdout/stderr, parsing JSON compiler
///   messages, and rendering them using [`crate::diagnostics::DiagnosticMapper`].
pub fn run_charon(
    args: &crate::resolve::Args,
    toolchain: &crate::setup::Toolchain,
    roots: &crate::resolve::LockedRoots,
    packages: &[crate::scanner::AnnealArtifact],
) -> anyhow::Result<()> {
    let llbc_root = roots.llbc_root();
    std::fs::create_dir_all(&llbc_root).context("Failed to create LLBC output directory")?;

    // Prepend to `PATH` to ensure that when `charon` delegates to tools such as
    // `cargo` and `rustc` it invokes the correct rust toolchain.
    let new_path = crate::util::prepend_to_env_var("PATH", &toolchain.rust_bin());

    let lib_env_var =
        if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" };
    // Set `LD_LIBRARY_PATH` (or `DYLD_LIBRARY_PATH` on macOS) to point to the
    // managed Rust toolchain's `lib` directory so that dynamic executables (like
    // `charon-driver` which links against `rustc` dynamic libraries) can find them.
    let new_lib_path = crate::util::prepend_to_env_var(lib_env_var, &toolchain.rust_lib());

    // Global print mutex to prevent interleaved printing of consolidated artifact buffers.
    let print_mutex = std::sync::Arc::new(std::sync::Mutex::new(()));

    // Determine if we should show progress bar.
    let show_progress = std::env::var("ANNEAL_NO_PROGRESS").is_err() && packages.len() == 1;

    packages.par_iter().try_for_each(|artifact| {
        if artifact.start_from.is_empty() {
            return Ok(());
        }

        log::info!("Invoking Charon on package '{}'...", artifact.name.package_name);

        let mut cmd = toolchain.command(crate::setup::Tool::Charon);

        // Set `CHARON_TOOLCHAIN_IS_IN_PATH=1` to instruct Charon to bypass its
        // standard toolchain resolution logic (which expects the compiler to be
        // managed via `rustup`). Instead, it will directly use the `rustc` and
        // `cargo` binaries we prepended to the `PATH` environment variable.
        cmd.env("CHARON_TOOLCHAIN_IS_IN_PATH", "1");
        cmd.env("PATH", &new_path);
        cmd.env(lib_env_var, &new_lib_path);

        cmd.arg("cargo");
        cmd.arg("--preset=aeneas");

        let llbc_path = artifact.llbc_path(roots);
        cmd.arg("--dest-file").arg(llbc_path);

        // Fail fast on errors.
        cmd.arg("--abort-on-error");

        for item in &artifact.items {
            if let crate::parse::ParsedItem::Function(func) = &item.item {
                // Check if the function body is an Axiom (unsafe).
                if let crate::parse::attr::FunctionBlockInner::Axiom { .. } = func.anneal.inner {
                    // Mark `unsafe(axiom)` functions as opaque in Charon. This
                    // instructs Aeneas to treat the function as external and
                    // generate a template file (`FunsExternal_Template.lean`)
                    // containing the type signature as an axiom, rather than
                    // attempting to translate the body. This allows users to
                    // mechanically verify the composition of safe code while
                    // manually vouching for the correctness of unsafe leaf
                    // functions.
                    let mut opaque_name = item.module_path.join("::");
                    opaque_name.push_str("::");
                    opaque_name.push_str(func.item.name());

                    log::trace!("Marking item as opaque in Charon: {}", opaque_name);
                    cmd.args(["--opaque", &opaque_name]);
                }
            }
        }

        // Start translation from specific entry points. Sort to ensure
        // deterministic ordering for testing. Determinism is important for
        // production too, because the order of arguments can affect the order
        // of generated definitions in Lean, which we want to be stable to
        // minimize churn.
        let mut start_from = artifact.start_from.iter().map(String::as_ref).collect::<Vec<_>>();
        start_from.sort_unstable(); // Slightly faster than `sort`, and equivalent for strings.

        if log::log_enabled!(log::Level::Trace) {
            for entry in &start_from {
                log::trace!("Translation entry point: {}", entry);
            }
        }

        let start_from_str = start_from.join(",");
        if start_from_str.len() > crate::util::ARG_CHAR_LIMIT {
            // FIXME: Pass the list of entry points to Charon via a file instead
            // of the command line.
            anyhow::bail!(
                "The list of Anneal entry points for package '{}' is too large ({} bytes). \n\
                 This exceeds safe OS command-line limits.",
                artifact.name.package_name,
                start_from_str.len()
            );
        }

        cmd.arg("--start-from").arg(start_from_str);

        // Separator for the underlying cargo command.
        cmd.arg("--");

        // Ensure cargo emits json msgs which charon-driver natively generates.
        cmd.arg("--message-format=json");

        cmd.arg("--manifest-path").arg(&artifact.manifest_path);

        use crate::resolve::AnnealTargetKind::*;
        match artifact.target_kind {
            Lib | RLib | ProcMacro | CDyLib | DyLib | StaticLib => cmd.arg("--lib"),
            Bin => cmd.args(["--bin", &artifact.name.target_name]),
            Example => cmd.args(["--example", &artifact.name.target_name]),
            Test => cmd.args(["--test", &artifact.name.target_name]),
        };

        // Forward all feature-related flags.
        if args.features.all_features {
            cmd.arg("--all-features");
        }
        if args.features.no_default_features {
            cmd.arg("--no-default-features");
        }
        for feature in &args.features.features {
            cmd.arg("--features").arg(feature);
        }

        // Reuse the main target directory for dependencies to save time: share
        // `CARGO_TARGET_DIR` (`target/anneal/cargo_target`) across all Charon
        // invocations to enable Cargo's incremental build cache.
        cmd.env("CARGO_TARGET_DIR", roots.cargo_target_dir());

        log::debug!("Executing charon command: {:?}", cmd);

        let start = std::time::Instant::now();
        let output_error = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let output_error_clone = std::sync::Arc::clone(&output_error);

        // Charon's standard error stream contains unstructured diagnostic
        // output (such as panic messages from build scripts or ICEs). We
        // collect this in a safety buffer to ensure that even if Charon aborts
        // unexpectedly, the user receives the complete unstructured output
        // instead of a generic "silent death".
        let safety_buffer = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let safety_buffer_clone = std::sync::Arc::clone(&safety_buffer);

        // Local buffer to collect all output (diagnostics) for this artifact.
        let artifact_diagnostics = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let artifact_diagnostics_clone = std::sync::Arc::clone(&artifact_diagnostics);

        let mut mapper = crate::diagnostics::DiagnosticMapper::new(roots.workspace().clone());

        let progress_msg = if show_progress { Some("Compiling...") } else { None };

        let res = crate::util::run_command_with_progress(cmd, progress_msg, move |line, pb| {
            if let Ok(msg) = serde_json::from_str::<cargo_metadata::Message>(line) {
                match msg {
                    cargo_metadata::Message::CompilerArtifact(a) => {
                        if let Some(p) = pb {
                            p.set_message(format!("Compiling {}", a.target.name));
                        }
                    }
                    cargo_metadata::Message::CompilerMessage(msg) => {
                        let mut rendered = String::new();
                        mapper.render_miette(&msg.message, |s| rendered.push_str(&s));
                        if !rendered.is_empty() {
                            artifact_diagnostics_clone.lock().unwrap().push(rendered);
                        }
                        if matches!(
                            msg.message.level,
                            cargo_metadata::diagnostic::DiagnosticLevel::Error | cargo_metadata::diagnostic::DiagnosticLevel::Ice
                        ) {
                            output_error_clone.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    cargo_metadata::Message::TextLine(t) => {
                        safety_buffer_clone.lock().unwrap().push(t);
                    }
                    _ => {}
                }
            } else {
                safety_buffer_clone.lock().unwrap().push(line.to_string());
            }
            Ok(())
        })?;

        log::trace!("Charon for '{}' took {:.2?}", artifact.name.package_name, start.elapsed());

        // Lock the print mutex to print this artifact's consolidated output atomically.
        let _lock = print_mutex.lock().unwrap();

        // Print all collected diagnostics for this artifact.
        let diags = artifact_diagnostics.lock().unwrap();
        if !diags.is_empty() {
            eprintln!("=== Diagnostics for '{}' ===", artifact.name.package_name);
            for diag in diags.iter() {
                eprintln!("{}", diag);
            }
        }

        if output_error.load(std::sync::atomic::Ordering::Relaxed) {
            anyhow::bail!("Diagnostic error in charon");
        } else if !res.status.success() {
            // Print safety buffer on failure (also atomically since we hold print_mutex).
            eprintln!("=== Failure output for '{}' ===", artifact.name.package_name);
            for line in safety_buffer.lock().unwrap().iter() {
                eprintln!("{}", line);
            }
            // Also dump the dynamic linker errors or panic messages captured in stderr.
            for line in &res.stderr_lines {
                eprintln!("{}", line);
            }
            anyhow::bail!("Charon failed with status: {}", res.status);
        }

        Ok(())
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use crate::resolve::{Args, resolve_roots};
    use crate::scanner::scan_workspace;
    use clap::Parser as _;

    #[test]
    fn test_run_charon_simple() {
        // Disable progress bar for test.
        unsafe { std::env::set_var("ANNEAL_NO_PROGRESS", "1"); }

        // 1. Create a temporary directory.
        let temp_dir = tempfile::tempdir().unwrap();
        let proj_dir = temp_dir.path().join("test_proj");
        fs::create_dir_all(&proj_dir).unwrap();

        // 2. Create a simple Cargo.toml.
        let cargo_toml = r#"
            [package]
            name = "test_proj"
            version = "0.1.0"
            edition = "2021"

            [lib]
            path = "src/lib.rs"
        "#;
        fs::write(proj_dir.join("Cargo.toml"), cargo_toml).unwrap();

        // 3. Create a simple src/lib.rs.
        fs::create_dir_all(proj_dir.join("src")).unwrap();
        let lib_rs = r#"
            pub fn add(left: usize, right: usize) -> usize {
                left + right
            }
        "#;
        fs::write(proj_dir.join("src").join("lib.rs"), lib_rs).unwrap();

        // 4. Construct Args pointing to this temp project.
        let args = Args::try_parse_from(&[
            "cargo-anneal",
            "--manifest-path",
            proj_dir.join("Cargo.toml").to_str().unwrap(),
        ]).unwrap();

        // 5. Resolve roots.
        let roots = resolve_roots(&args).unwrap();

        // 6. Scan workspace (our simplified scanner will find `add` function).
        let packages = scan_workspace(&roots).unwrap();
        assert_eq!(packages.len(), 1);
        assert!(packages[0].start_from.contains("crate::add"));

        // 7. Lock run root.
        let locked_roots = roots.lock_run_root().unwrap();

        // 8. Resolve test-only stripped toolchain and run charon.
        let toolchain_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("toolchains")
            .join("charon-only");
        let toolchain = crate::setup::Toolchain::new_test(toolchain_root);

        let res = run_charon(&args, &toolchain, &locked_roots, &packages);
        assert!(res.is_ok(), "charon failed: {:?}", res.err());

        // 9. Verify .llbc file exists.
        let llbc_path = packages[0].llbc_path(&locked_roots);
        assert!(llbc_path.exists(), "llbc file not found at {:?}", llbc_path);

        unsafe { std::env::remove_var("ANNEAL_NO_PROGRESS"); }
    }
}
