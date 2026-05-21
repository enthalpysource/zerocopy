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

/// Runs Charon on the specified packages to generate LLBC artifacts.
///
/// This function requires `LockedRoots` to ensure that it has exclusive access
/// to the `llbc` output directory. It iterates over each `AnnealArtifact`,
/// constructs the appropriate `charon` command, and executes it.
///
/// It handles:
/// - **Opaque Functions**: identifying `unsafe(axiom)` functions and passing
///   `--opaque` to Charon.
/// - **Entry Points**: passing the computed `start_from` roots to Charon to
///   minimize extraction scope.
/// - **Output Handling**: capturing stdout/stderr, parsing JSON compiler
///   messages, and rendering them using `DiagnosticMapper`.
pub fn run_charon(
    args: &crate::resolve::Args,
    roots: &crate::resolve::LockedRoots,
    packages: &[crate::scanner::AnnealArtifact],
    toolchain: &crate::setup::Toolchain,
) -> anyhow::Result<()> {
    let llbc_root = roots.llbc_root();
    std::fs::create_dir_all(&llbc_root).context("Failed to create LLBC output directory")?;

    let rust_bin = toolchain.rust_bin();
    let rust_lib = toolchain.rust_lib();

    // We prepend the managed Rust toolchain's `bin` directory to `PATH`. This is
    // necessary because Charon is a compiler frontend that invokes `cargo` and
    // `rustc` under the hood. To ensure hermeticity and correctness, we must force
    // Charon to use our pinned nightly compiler version rather than falling back
    // to whatever version happens to be installed in the system `PATH`.
    let new_path = crate::util::prepend_to_env_var("PATH", rust_bin);

    let lib_env_var =
        if cfg!(target_os = "macos") { "DYLD_LIBRARY_PATH" } else { "LD_LIBRARY_PATH" };
    // We set `LD_LIBRARY_PATH` (or `DYLD_LIBRARY_PATH` on macOS) to point to the
    // managed Rust toolchain's `lib` directory. This is strictly required because
    // `charon-driver` is a dynamic executable that links against `rustc` compiler
    // dynamic libraries (like `librustc_driver-*.so`). Without this, the dynamic
    // linker will fail to load the libraries when `charon-driver` is executed.
    let new_lib_path = crate::util::prepend_to_env_var(lib_env_var, rust_lib);

    for artifact in packages {
        if artifact.start_from.is_empty() {
            continue;
        }

        log::info!("Invoking Charon on package '{}'...", artifact.name.package_name);

        let mut cmd = toolchain.command(crate::setup::Tool::Charon);

        // We set `CHARON_TOOLCHAIN_IS_IN_PATH=1` to instruct Charon to bypass its
        // standard toolchain resolution logic (which expects the compiler to be
        // managed via `rustup`). Instead, it will directly use the `rustc` and
        // `cargo` binaries we prepended to the `PATH` environment variable.
        cmd.env("CHARON_TOOLCHAIN_IS_IN_PATH", "1");
        cmd.env("PATH", &new_path);
        cmd.env(lib_env_var, &new_lib_path);

        cmd.arg("cargo");
        cmd.arg("--preset=aeneas");

        // Output artifacts to target/anneal/<hash>/llbc.
        let llbc_path = artifact.llbc_path(roots);
        log::debug!("Writing .llbc file to {:?}", llbc_path);
        cmd.arg("--dest-file").arg(llbc_path);

        // We use `--abort-on-error` to fail fast. If Charon (or `rustc`)
        // encounters a compilation error or translation failure (e.g., an
        // unsupported Rust feature), it will terminate the process immediately
        // rather than attempting to proceed and translate other parts of the crate.
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

        match artifact.target_kind {
            crate::resolve::AnnealTargetKind::Lib | crate::resolve::AnnealTargetKind::RLib | crate::resolve::AnnealTargetKind::ProcMacro | crate::resolve::AnnealTargetKind::CDyLib | crate::resolve::AnnealTargetKind::DyLib | crate::resolve::AnnealTargetKind::StaticLib => cmd.arg("--lib"),
            crate::resolve::AnnealTargetKind::Bin => cmd.args(["--bin", &artifact.name.target_name]),
            crate::resolve::AnnealTargetKind::Example => cmd.args(["--example", &artifact.name.target_name]),
            crate::resolve::AnnealTargetKind::Test => cmd.args(["--test", &artifact.name.target_name]),
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

        // We share `CARGO_TARGET_DIR` (`target/anneal/cargo_target`) across all
        // Charon invocations to enable Cargo's incremental build cache. This
        // prevents Cargo from recompiling identical workspace dependencies from
        // scratch for each individual target, saving significant compilation time.
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

        let mut mapper = crate::diagnostics::DiagnosticMapper::new(roots.workspace().clone());

        let res = crate::util::run_command_with_progress(cmd, "Compiling...", move |line, pb| {
            if let Ok(msg) = serde_json::from_str::<cargo_metadata::Message>(line) {
                match msg {
                    cargo_metadata::Message::CompilerArtifact(a) => {
                        pb.set_message(format!("Compiling {}", a.target.name));
                    }
                    cargo_metadata::Message::CompilerMessage(msg) => {
                        pb.suspend(|| {
                            mapper.render_miette(&msg.message, |s| eprintln!("{}", s));
                        });
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

        // FIXME: There's a subtle edge case here – if we get error output AND
        // Rustc ICE's, there's a good chance that the JSON error messages we
        // print won't include all relevant information – some will be printed
        // via stderr. In this case, `output_error = true` and so we bail and
        // discard stderr, which will swallow information from the user.
        if output_error.load(std::sync::atomic::Ordering::Relaxed) {
            anyhow::bail!("Diagnostic error in charon");
        } else if !res.status.success() {
            // "Silent Death" dump.
            for line in safety_buffer.lock().unwrap().iter() {
                eprintln!("{}", line);
            }
            // Also dump the dynamic linker errors or panic messages captured in stderr.
            for line in &res.stderr_lines {
                eprintln!("{}", line);
            }
            anyhow::bail!("Charon failed with status: {}", res.status);
        }
    }

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
        // 1. Create a temporary directory
        let temp_dir = tempfile::tempdir().unwrap();
        let proj_dir = temp_dir.path().join("test_proj");
        fs::create_dir_all(&proj_dir).unwrap();

        // 2. Create a simple Cargo.toml
        let cargo_toml = r#"
            [package]
            name = "test_proj"
            version = "0.1.0"
            edition = "2021"

            [lib]
            path = "src/lib.rs"
        "#;
        fs::write(proj_dir.join("Cargo.toml"), cargo_toml).unwrap();

        // 3. Create a simple src/lib.rs
        fs::create_dir_all(proj_dir.join("src")).unwrap();
        let lib_rs = r#"
            pub fn add(left: usize, right: usize) -> usize {
                left + right
            }
        "#;
        fs::write(proj_dir.join("src").join("lib.rs"), lib_rs).unwrap();

        // 4. Construct Args pointing to this temp project
        let args = Args::try_parse_from(&[
            "cargo-anneal",
            "--manifest-path",
            proj_dir.join("Cargo.toml").to_str().unwrap(),
        ]).unwrap();

        // 5. Resolve roots
        let roots = resolve_roots(&args).unwrap();

        // 6. Scan workspace (our simplified scanner will find `add` function)
        let packages = scan_workspace(&roots).unwrap();
        assert_eq!(packages.len(), 1);
        assert!(packages[0].start_from.contains("crate::add"));

        // 7. Lock run root
        let locked_roots = roots.lock_run_root().unwrap();

        // 8. Resolve test-only stripped toolchain and run charon
        let toolchain_root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("toolchains")
            .join("charon-only");
        let toolchain = crate::setup::Toolchain::new_test(toolchain_root);

        let res = run_charon(&args, &locked_roots, &packages, &toolchain);
        assert!(res.is_ok(), "charon failed: {:?}", res.err());

        // 9. Verify .llbc file exists
        let llbc_path = packages[0].llbc_path(&locked_roots);
        assert!(llbc_path.exists(), "llbc file not found at {:?}", llbc_path);
    }
}
