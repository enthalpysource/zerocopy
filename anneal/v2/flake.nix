{
  description = "Clean Aeneas Downloader Derivation";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = import nixpkgs {
          inherit system;
          config.allowUnfree = true;
        };

        # Upstream platform names.
        rustPlatform = if system == "x86_64-linux" then "x86_64-unknown-linux-gnu"
                       else if system == "aarch64-linux" then "aarch64-unknown-linux-gnu"
                       else if system == "x86_64-darwin" then "x86_64-apple-darwin"
                       else if system == "aarch64-darwin" then "aarch64-apple-darwin"
                       else throw "Unsupported system: ${system}";

        leanPlatform = if system == "x86_64-linux" then "linux"
                       else if system == "aarch64-linux" then "linux_aarch64"
                       else if system == "x86_64-darwin" then "darwin"
                       else if system == "aarch64-darwin" then "darwin_aarch64"
                       else throw "Unsupported system: ${system}";

        # Prebuilt Aeneas release archive.
        fetchAeneas = { target, releaseTag, sha256 }:
          pkgs.fetchurl {
            name = "aeneas-${target}.tar.gz";
            url = "https://github.com/AeneasVerif/aeneas/releases/download/${releaseTag}/aeneas-${target}.tar.gz";
            inherit sha256;
          };

        # Fixed-output downloader used for toolchain assets.
        fetchToolchainAsset = { pname, version, sha256, buildPhase }:
          pkgs.stdenv.mkDerivation {
            inherit pname version sha256 buildPhase;

            dontUnpack = true;

            # Keep downloaded toolchains byte-for-byte independent of the builder.
            dontPatchShebangs = true;
            dontPatchELF = true;
            dontStrip = true;

            outputHashMode = "recursive";
            outputHashAlgo = "sha256";
            outputHash = sha256;

            nativeBuildInputs = with pkgs; [
              curl
              cacert
              gnutar
              gzip
              zstd
            ];

            SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";
          };

        # Merge the Rust components into one sysroot.
        fetchRustToolchain = { rustDate, sha256 }:
          fetchToolchainAsset {
            pname = "rust-toolchain";
            version = rustDate;
            inherit sha256;

            buildPhase = builtins.concatStringsSep "\n" [
              "mkdir -p $out"
              # Rust archives nest each component under a top-level directory.
              "extract_component() {"
              "  local name=$1"
              "  local url=\"https://static.rust-lang.org/dist/${rustDate}/\${name}-nightly-${rustPlatform}.tar.gz\""
              "  echo \"Downloading and extracting $name from $url...\""
              "  mkdir -p tmp_extract"
              "  curl -sSL \"$url\" | tar -xz -C tmp_extract"
              "  local top_dir=$(ls tmp_extract | head -n 1)"
              "  local comp_dir=$(find \"tmp_extract/$top_dir\" -mindepth 1 -maxdepth 1 -type d | head -n 1)"
              "  cp -r $comp_dir/* $out/"
              "  rm -rf tmp_extract"
              "}"
              "extract_component \"rustc\""
              "extract_component \"rust-std\""
              "extract_component \"rustc-dev\""
              "extract_component \"llvm-tools\""
              "extract_component \"miri\""
              "echo \"Downloading and extracting rust-src...\""
              "mkdir -p tmp_extract"
              "curl -sSL \"https://static.rust-lang.org/dist/${rustDate}/rust-src-nightly.tar.gz\" | tar -xz -C tmp_extract"
              "local top_dir=$(ls tmp_extract | head -n 1)"
              "cp -r tmp_extract/$top_dir/rust-src/* $out/"
              "rm -rf tmp_extract"
            ];
          };

        # Download and unpack the Lean compiler toolchain.
        fetchLeanToolchain = { leanVersion, sha256 }:
          let
            # Lean archives omit the leading "v" in their filenames.
            rawVersion = if builtins.substring 0 1 leanVersion == "v"
                         then builtins.substring 1 (builtins.stringLength leanVersion - 1) leanVersion
                         else leanVersion;
          in
          fetchToolchainAsset {
            pname = "lean-toolchain";
            version = rawVersion;
            inherit sha256;

            buildPhase = builtins.concatStringsSep "\n" [
              "mkdir -p $out"
              "url=\"https://releases.lean-lang.org/lean4/${leanVersion}/lean-${rawVersion}-${leanPlatform}.tar.zst\""
              "echo \"Downloading Lean toolchain from $url...\""
              "curl -sSL \"$url\" | zstd -d | tar -x -C $out --strip-components=1"
            ];
          };
      in
      {
        # FIXME: The output set is evaluated for every flake-utils default
        # system, but the omnibus archive is currently only real for x86_64
        # Linux. True multi-platform archives need per-system Aeneas release
        # assets and hashes, per-system Lean/Rust archive hashes, and
        # platform-specific binary cleanup instead of unconditional Linux ELF
        # interpreter patching.
        packages.aeneas-download-linux-x86_64 = fetchAeneas {
          target = "linux-x86_64";
          releaseTag = "nightly-2026.06.03";
          sha256 = "00fb8ef427d4d06dcabd90f5196266e07731adf2a7466964ce6f4f6d1e8cbc11";
        };

        # Extracts the toolchain metadata implied by the Aeneas archive.
        packages.aeneas-unpacked = pkgs.stdenv.mkDerivation {
          pname = "aeneas-unpacked";
          version = "1.0.0";

          src = self.packages.${system}.aeneas-download-linux-x86_64;

          nativeBuildInputs = with pkgs; [
            gnutar
            gzip
            binutils  # Provides `strings` utility
          ];

          dontUnpack = true;

          buildPhase = builtins.concatStringsSep "\n" [
            "mkdir -p $out"
            "tar -xzf $src -C $out"
            "chmod -R +w $out"
            "LEAN_RAW=\$(cat $out/backends/lean/lean-toolchain)"
            "LEAN_VERSION=\$(echo \"\$LEAN_RAW\" | sed -E 's|leanprover/lean4:v?||' | tr -d '\\n')"
            # Charon embeds the rustc commit date; nightly is the following day.
            # FIXME: prefer an upstream pin file if Aeneas publishes one.
            "COMMIT_DATE=\$(strings $out/charon | grep -o \"rustc version .* ([0-9a-f]\\{9\\} [0-9]\\{4\\}-[0-9]\\{2\\}-[0-9]\\{2\\})\" | head -n 1 | grep -o \"[0-9]\\{4\\}-[0-9]\\{2\\}-[0-9]\\{2\\}\" | tr -d '\\n')"
            "RUST_DATE=\$(date -d \"\$COMMIT_DATE + 1 day\" +%Y-%m-%d)"
            "RUST_VERSION=\"nightly-\$RUST_DATE\""
            "cat <<EOF > $out/metadata.json"
            "{"
            "  \"lean-toolchain\": \"\$LEAN_VERSION\","
            "  \"rust-toolchain-date\": \"\$RUST_DATE\","
            "  \"rust-toolchain-version\": \"\$RUST_VERSION\""
            "}"
            "EOF"
          ];
        };

        # Minimal project metadata used to fetch the Mathlib cache.
        packages.aeneas-metadata-files = pkgs.stdenv.mkDerivation {
          pname = "aeneas-metadata-files";
          version = "1.0.0";

          src = self.packages.${system}.aeneas-download-linux-x86_64;

          nativeBuildInputs = with pkgs; [
            gnutar
            gzip
          ];

          dontUnpack = true;

          buildPhase = builtins.concatStringsSep "\n" [
            "mkdir -p $out"
            "tar -xzf $src -C $out --strip-components=2 \\"
            "  backends/lean/lakefile.lean \\"
            "  backends/lean/lake-manifest.json \\"
            "  backends/lean/lean-toolchain"
          ];
        };

        # Fetches Mathlib's precompiled Lake cache in a fixed-output derivation.
        packages.mathlib-cache-download = pkgs.stdenv.mkDerivation {
          pname = "mathlib-cache-download";
          version = "0.1.0";

          dontUnpack = true;

          # Preserve downloaded artifacts exactly.
          dontPatchShebangs = true;
          dontPatchELF = true;
          dontStrip = true;

          outputHashMode = "recursive";
          outputHashAlgo = "sha256";
          outputHash = "sha256-tj5BeYGWSmXvYdzChrqaG9aN7so/+jAM5qWPI8tcN6U=";

          leanToolchainRaw = self.packages.${system}.lean-toolchain;
          metadataFiles = self.packages.${system}.aeneas-metadata-files;

          nativeBuildInputs = with pkgs; [
            git
            steam-run
            gnutar
            zstd
            curl
            cacert
          ];

          SSL_CERT_FILE = "${pkgs.cacert}/etc/ssl/certs/ca-bundle.crt";

          buildPhase = builtins.concatStringsSep "\n" [
            "export HOME=$TMPDIR"
            "mkdir -p project"
            "cp $metadataFiles/lakefile.lean project/"
            "cp $metadataFiles/lake-manifest.json project/"
            "cp $metadataFiles/lean-toolchain project/"
            "cd project"
            "export PATH=\"$leanToolchainRaw/bin:${pkgs.git}/bin:${pkgs.curl}/bin:\$PATH\""
            "export LEAN_SYSROOT=\"$leanToolchainRaw\""
            "steam-run $leanToolchainRaw/bin/lake exe cache get"
          ];

          installPhase = builtins.concatStringsSep "\n" [
            "mkdir -p $out/cache/mathlib"
            "cp -r $TMPDIR/.cache/mathlib/* $out/cache/mathlib/"
            "mkdir -p $out/packages"
            "cp -r .lake/packages/* $out/packages/"
            "chmod -R +w $out/packages"
            # Drop only traces that captured Nix store paths.
            "find $out/packages -type f \\( -name \"*.trace\" -o -name \"*.hash\" \\) \\"
            "  -exec grep -q \"/nix/store\" {} \\; -delete"
            # Mathlib's build cache is reconstructed from .ltar archives below.
            "rm -rf $out/packages/mathlib/.lake"
            # Git metadata is unnecessary for path dependencies.
            "find $out/packages -type d -name \".git\" -exec rm -rf {} +"
          ];
        };

        # Unpacks Mathlib's precompiled .ltar archives.
        packages.mathlib-cache-unpacked = pkgs.stdenv.mkDerivation {
          pname = "mathlib-cache-unpacked";
          version = "0.1.0";

          src = pkgs.runCommand "empty-src" {} "mkdir $out";

          mathlibCache = self.packages.${system}.mathlib-cache-download;
          leanToolchain = self.packages.${system}.lean-toolchain;

          nativeBuildInputs = with pkgs; [
            gnutar
            zstd
          ];

          buildPhase = builtins.concatStringsSep "\n" [
            "mkdir -p $out/packages"
            "cp -r $mathlibCache/packages/* $out/packages/"
            "chmod -R +w $out/packages"
            "LEANTAR_BIN=\$(find $mathlibCache/cache/mathlib -name \"leantar-*\" -type f -executable | head -n 1)"
            "if [ -z \"\$LEANTAR_BIN\" ] && [ -x \"$leanToolchain/bin/leantar\" ]; then"
            "  LEANTAR_BIN=\"$leanToolchain/bin/leantar\""
            "fi"
            "if [ -z \"\$LEANTAR_BIN\" ]; then"
            "  echo \"ERROR: leantar utility binary not found in mathlib cache or Lean toolchain!\""
            "  exit 1"
            "fi"
            "echo \"Using leantar binary at: \$LEANTAR_BIN\""
            # Each archive expands into the project-wide .lake/build tree.
            "find $mathlibCache/cache/mathlib -name \"*.ltar\" -print0 | \\"
            "  xargs -0 -n 1 -P 48 bash -c \"\$LEANTAR_BIN -d -C $out \\\"\\\$0\\\"\""
            # Keep the release archive reproducible.
            "find $out -exec touch -h -d \"1970-01-01 00:00:00\" {} +"
          ];
        };

        # Builds the Aeneas Lean backend against vendored, relative Lake paths.
        packages.aeneas-compiled = pkgs.stdenv.mkDerivation {
          pname = "aeneas-compiled";
          version = "0.1.0";

          src = pkgs.runCommand "empty-src" {} "mkdir $out";

          leanToolchain = self.packages.${system}.lean-toolchain;
          mathlibCache = self.packages.${system}.mathlib-cache-unpacked;
          aeneasUnpacked = self.packages.${system}.aeneas-unpacked;

          nativeBuildInputs = with pkgs; [
            python3
            steam-run
            gnutar
            zstd
          ];

          buildPhase = builtins.concatStringsSep "\n" [
            "export HOME=$TMPDIR"
            "export PATH=\"$leanToolchain/bin:\$PATH\""
            "export LEAN_SYSROOT=\"$leanToolchain\""
            # Let sandboxed Lean executables find libleanshared.so.
            "export LD_LIBRARY_PATH=\"$leanToolchain/lib:$leanToolchain/lib/lean:\$LD_LIBRARY_PATH\""
            # The cache was fetched in the FOD; do not fetch it again here.
            "export MATHLIB_NO_CACHE_ON_UPDATE=1"
            "mkdir -p aeneas/backends aeneas/packages"
            "cp -r $aeneasUnpacked/backends/lean aeneas/backends/lean"
            "chmod -R +w aeneas"
            "cd aeneas/backends/lean"
            "cp -r $mathlibCache/packages/* ../../packages/"
            "chmod -R +w ../../packages"
            # Seed the project-wide build cache from unpacked Mathlib archives.
            "mkdir -p .lake"
            "cp -r $mathlibCache/.lake/build .lake/"
            "chmod -R +w .lake"
            # Copy global prebuilts into each path dependency for offline replay.
            ''
              python3 << 'EOF'
              import os
              import shutil
              import re

              def discover_library_name(pkg_dir, dir_name):
                  toml_path = os.path.join(pkg_dir, "lakefile.toml")
                  if os.path.exists(toml_path):
                      with open(toml_path, "r") as f:
                          content = f.read()
                      match = re.search(r"\[{1,2}lean_lib\]{1,2}(?:\s*\n|[^\[\]])*?name\s*=\s*\"([^\"]+)\"", content)
                      if match:
                          return match.group(1)
                      match = re.search(r"^\s*name\s*=\s*\"([^\"]+)\"", content, re.MULTILINE)
                      if match:
                          return match.group(1).capitalize()

                  lean_path = os.path.join(pkg_dir, "lakefile.lean")
                  if os.path.exists(lean_path):
                      with open(lean_path, "r") as f:
                          content = f.read()
                      match = re.search(r"lean_lib\s+(?:«([^»]+)»|\"([^\"]+)\"|([a-zA-Z0-9._-]+))", content)
                      if match:
                          return match.group(1) or match.group(2) or match.group(3)
                      match = re.search(r"package\s+(?:«([^»]+)»|\"([^\"]+)\"|([a-zA-Z0-9._-]+))", content)
                      if match:
                          val = match.group(1) or match.group(2) or match.group(3)
                          return val.capitalize()

                  return dir_name.capitalize()

              global_cache = os.path.join(os.getcwd(), ".lake", "build")
              packages_dir = os.path.abspath("../../packages")

              if os.path.exists(packages_dir):
                  for dir_name in os.listdir(packages_dir):
                      pkg_dir = os.path.join(packages_dir, dir_name)
                      if not os.path.isdir(pkg_dir): continue
                      pkg_name = discover_library_name(pkg_dir, dir_name)
                      dest_lib_dir = os.path.join(pkg_dir, ".lake", "build", "lib", "lean")
                      os.makedirs(dest_lib_dir, exist_ok=True)
                      src_lib_dir = os.path.join(global_cache, "lib", "lean", pkg_name)
                      if os.path.exists(src_lib_dir):
                          shutil.copytree(src_lib_dir, os.path.join(dest_lib_dir, pkg_name), dirs_exist_ok=True)
                      src_root_lib_dir = os.path.join(global_cache, "lib", "lean")
                      if os.path.exists(src_root_lib_dir):
                          for f in os.listdir(src_root_lib_dir):
                              if f.startswith(pkg_name + "."):
                                  shutil.copy2(os.path.join(src_root_lib_dir, f), dest_lib_dir)
                      dest_ir_dir = os.path.join(pkg_dir, ".lake", "build", "ir")
                      src_ir_dir = os.path.join(global_cache, "ir", pkg_name)
                      if os.path.exists(src_ir_dir):
                          os.makedirs(dest_ir_dir, exist_ok=True)
                          shutil.copytree(src_ir_dir, os.path.join(dest_ir_dir, pkg_name), dirs_exist_ok=True)
              EOF
            ''
            "python3 ${./rewrite-lake-vendor.py} --root . --packages-dir ../../packages"
            "steam-run lake build"
            "python3 ${./rewrite-lake-vendor.py} --root . --packages-dir ../../packages --rewrite-traces --trace-prefix \"$leanToolchain=lean\""
            "TRACE_ABS_RE='(^|[\"[:space:]=:])/(nix/store|build|private/tmp/nix-build|ANNEAL_PLACEHOLDER_ROOT)'"
            "if find . ../../packages -type f -name \"*.trace\" -exec grep -EIl \"\$TRACE_ABS_RE\" {} + | tee /tmp/non-relocatable-traces | grep -q .; then"
            "  echo \"ERROR: non-relocatable paths remain in Lake trace files\" >&2"
            "  cat /tmp/non-relocatable-traces >&2"
            "  exit 1"
            "fi"
            # Prune unused Lean modules and bulky upstream metadata.
            ''
              python3 << 'EOF'
              import os
              import shutil

              def prune_package(pkg_dir):
                  print(f"Pruning package at: {pkg_dir}")
                  traces_dir = os.path.join(pkg_dir, ".lake", "build", "lib", "lean")
                  if not os.path.exists(traces_dir):
                      print(f"No build traces found for {pkg_dir}, skipping pruning.")
                      return
                  used_modules = set()
                  for root, dirs, files in os.walk(traces_dir):
                      for file in files:
                          if file.endswith(".trace"):
                              abs_trace = os.path.join(root, file)
                              rel_trace = os.path.relpath(abs_trace, traces_dir)
                              mod_path = os.path.splitext(rel_trace)[0]
                              used_modules.add(mod_path)
                  all_lean_files = []
                  for root, dirs, files in os.walk(pkg_dir):
                      if ".lake" in os.path.split(root)[1] or ".git" in os.path.split(root)[1]:
                          continue
                      for file in files:
                          if file.endswith(".lean"):
                              abs_lean = os.path.join(root, file)
                              rel_lean = os.path.relpath(abs_lean, pkg_dir)
                              all_lean_files.append(rel_lean)
                  print(f"  - Found {len(all_lean_files)} total .lean files.")
                  if os.path.basename(pkg_dir) != "mathlib":
                      used_modules.update(os.path.splitext(rel)[0] for rel in all_lean_files)
                      print("  - Keeping all Lean modules for non-Mathlib package.")
                  print(f"  - Keeping {len(used_modules)} modules.")
                  unused_count = 0
                  for rel_lean in all_lean_files:
                      if rel_lean == "lakefile.lean":
                          continue
                      mod_path = os.path.splitext(rel_lean)[0]
                      if mod_path not in used_modules:
                          abs_lean = os.path.join(pkg_dir, rel_lean)
                          os.remove(abs_lean)
                          unused_count += 1
                          for sub in ["lib/lean", "ir"]:
                              sub_dir = os.path.join(pkg_dir, ".lake", "build", sub)
                              if os.path.exists(sub_dir):
                                  mod_dir = os.path.join(sub_dir, os.path.dirname(mod_path))
                                  mod_name = os.path.basename(mod_path)
                                  if os.path.exists(mod_dir):
                                      for f in os.listdir(mod_dir):
                                          if f.startswith(mod_name + "."):
                                              os.remove(os.path.join(mod_dir, f))
                  print(f"  - Deleted {unused_count} unused .lean files and their build artifacts.")
                  meta_names = [".gitignore", ".gitattributes", "README.md", "LICENSE", "bors.toml", ".pre-commit-config.yaml", ".gitpod.yml"]
                  for name in meta_names:
                      p = os.path.join(pkg_dir, name)
                      if os.path.exists(p): os.remove(p)
                  meta_dirs = [".github", ".vscode", ".devcontainer", ".docker", "docs", "tests", "test"]
                  for name in meta_dirs:
                      p = os.path.join(pkg_dir, name)
                      if os.path.exists(p): shutil.rmtree(p, ignore_errors=True)
                  for root, dirs, files in os.walk(pkg_dir):
                      for file in files:
                          if file.endswith(".ltar"): os.remove(os.path.join(root, file))
                  for root, dirs, files in os.walk(pkg_dir, topdown=False):
                      if root == pkg_dir: continue
                      if not os.listdir(root): os.rmdir(root)

              packages_root = os.path.abspath("../../packages")
              if os.path.exists(packages_root):
                  for name in os.listdir(packages_root):
                      pkg_dir = os.path.join(packages_root, name)
                      if os.path.isdir(pkg_dir):
                          prune_package(pkg_dir)

              root_mathlib_build = os.path.join(os.getcwd(), ".lake", "build", "lib", "lean", "Mathlib")
              if os.path.exists(root_mathlib_build):
                  print("Deleting root Mathlib build cache...")
                  shutil.rmtree(root_mathlib_build)
              lib_lean_dir = os.path.join(os.getcwd(), ".lake", "build", "lib", "lean")
              if os.path.exists(lib_lean_dir):
                  for f in os.listdir(lib_lean_dir):
                      if f.startswith("Mathlib."):
                          os.remove(os.path.join(lib_lean_dir, f))
              EOF
            ''
            "cd ../.."
            "mkdir -p $out/backends $out/packages"
            "cp -r backends/lean $out/backends/"
            "cp -r packages/* $out/packages/"
            "mkdir -p $out/bin"
            "cp \$(find $aeneasUnpacked -maxdepth 1 -type f -executable) $out/bin/"
          ];
        };

        # Stages the relocatable toolchain bundle before compression.
        packages.omnibus-tar = pkgs.stdenv.mkDerivation {
          pname = "anneal-toolchain-omnibus-tar";
          version = "0.1.0";

          src = pkgs.runCommand "empty-src" {} "mkdir $out";

          nativeBuildInputs = with pkgs; [
            patchelf
            file
            gnutar
          ];

          aeneasBuild = self.packages.${system}.aeneas-compiled;
          rustToolchain = self.packages.${system}.rust-toolchain;
          leanToolchain = self.packages.${system}.lean-toolchain;

          buildPhase = builtins.concatStringsSep "\n" [
            "mkdir -p $TMPDIR/dist_staging"
            "chmod -R +w $TMPDIR/dist_staging/"
            "mkdir -p $TMPDIR/dist_staging/lean"
            "cp -r $leanToolchain/* $TMPDIR/dist_staging/lean/"
            "chmod -R +w $TMPDIR/dist_staging/lean"
            "mkdir -p $TMPDIR/dist_staging/rust"
            "cp -r $rustToolchain/* $TMPDIR/dist_staging/rust/"
            "chmod -R +w $TMPDIR/dist_staging/rust"
            "mkdir -p $TMPDIR/dist_staging/aeneas"
            "cp -r $aeneasBuild/* $TMPDIR/dist_staging/aeneas/"
            "chmod -R +w $TMPDIR/dist_staging/aeneas"
            # Remove Nix dynamic-linker and RPATH references from ELF binaries.
            "echo \"Cleaning up Nix store references...\""
            "find $TMPDIR/dist_staging -type f -executable | while read -r file; do"
            "  if file \"\$file\" | grep -q \"ELF 64-bit\"; then"
            "    echo \"Patching and stripping \$file...\""
            "    if patchelf --print-interpreter \"\$file\" >/dev/null 2>&1; then"
            "      patchelf --set-interpreter /lib64/ld-linux-x86-64.so.2 \"\$file\" || true"
            "    fi"
            "    patchelf --set-rpath \"\" \"\$file\" || true"
            "    strip \"\$file\" || true"
            "  fi"
            "done"
            "TRACE_ABS_RE='(^|[\"[:space:]=:])/(nix/store|build|private/tmp/nix-build|ANNEAL_PLACEHOLDER_ROOT)'"
            "if find $TMPDIR/dist_staging -type f -name \"*.trace\" -exec grep -EIl \"\$TRACE_ABS_RE\" {} + | tee /tmp/non-relocatable-staged-traces | grep -q .; then"
            "  echo \"ERROR: non-relocatable paths remain in staged Lake trace files\" >&2"
            "  cat /tmp/non-relocatable-staged-traces >&2"
            "  exit 1"
            "fi"
            "chmod -R a-w $TMPDIR/dist_staging"
            "cd $TMPDIR/dist_staging"
            "tar -cf $out *"
          ];
        };

        # Final compressed toolchain archive. This is the local-development
        # default, so keep compression fast.
        packages.omnibus-archive = pkgs.stdenv.mkDerivation {
          pname = "anneal-toolchain-omnibus";
          version = "0.1.0";

          src = pkgs.runCommand "empty-src" {} "mkdir $out";

          nativeBuildInputs = with pkgs; [
            zstd
          ];

          omnibusTar = self.packages.${system}.omnibus-tar;

          ANNEAL_ZSTD_LEVEL = 1;

          buildPhase = builtins.concatStringsSep "\n" [
            "ZSTD_LEVEL=\${ANNEAL_ZSTD_LEVEL:-1}"
            "echo \"Compressing with Zstd level \$ZSTD_LEVEL...\""
            "zstd -\$ZSTD_LEVEL $omnibusTar -o $out"
          ];
        };

        # CI usually substitutes this archive from the default-branch Nix
        # cache, so optimize the CI output for download size rather than for
        # the rare from-scratch compression path.
        packages.omnibus-archive-ci = self.packages.${system}.omnibus-archive.overrideAttrs (_: {
          ANNEAL_ZSTD_LEVEL = 19;
        });

        packages.rust-toolchain = fetchRustToolchain {
          rustDate = "2026-05-31";
          sha256 = "sha256-tdLBvDewiNTUKOdMJ1pkU7mPrUY0xTFOZWdG9dDNiAk=";
        };

        packages.lean-toolchain = fetchLeanToolchain {
          leanVersion = "v4.30.0-rc2";
          sha256 = "sha256-o47cQjSLK5YL8YZ2raaj+mGAvvO+dIDfVeP2L+WoyMs=";
        };

        # Verifies that Aeneas metadata can drive toolchain derivations.
        packages.test-ifd =
          let
            unpacked = self.packages.${system}.aeneas-unpacked;

            aeneasMetadata = builtins.fromJSON (builtins.readFile "${unpacked}/metadata.json");

            leanVersion = aeneasMetadata.lean-toolchain;
            rustVersion = aeneasMetadata.rust-toolchain-version;
            rustDate = aeneasMetadata.rust-toolchain-date;

            dynamicRust = fetchRustToolchain {
              inherit rustDate;
              sha256 = self.packages.${system}.rust-toolchain.outputHash;
            };

            dynamicLean = fetchLeanToolchain {
              inherit leanVersion;
              sha256 = self.packages.${system}.lean-toolchain.outputHash;
            };
          in
          pkgs.runCommand "test-ifd-eval" {} (builtins.concatStringsSep "\n" [
            "echo \"Dynamic IFD Verification Success!\""
            "echo \"Extracted Lean Toolchain Version: ${leanVersion}\""
            "echo \"Extracted Rust Toolchain Version: ${rustVersion}\""
            "echo \"Dynamically Constructed Rust Toolchain Store Path: ${dynamicRust}\""
            "echo \"Dynamically Constructed Lean Toolchain Store Path: ${dynamicLean}\""
            "test -f ${dynamicRust}/bin/rustc"
            "test -f ${dynamicLean}/bin/lean"
            "echo \"Lean: ${leanVersion}, Rust: ${rustVersion}\" > $out"
            "echo \"Wired Rust Toolchain: ${dynamicRust}\" >> $out"
            "echo \"Wired Lean Toolchain: ${dynamicLean}\" >> $out"
          ]);

        packages.default = self.packages.${system}.aeneas-unpacked;
      });
}
