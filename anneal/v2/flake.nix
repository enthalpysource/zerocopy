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

        aeneasTarget = if system == "x86_64-linux" then "linux-x86_64"
                       else if system == "aarch64-linux" then "linux-aarch64"
                       else if system == "x86_64-darwin" then "macos-x86_64"
                       else if system == "aarch64-darwin" then "macos-aarch64"
                       else throw "Unsupported system: ${system}";

        aeneasSha256 = if system == "x86_64-linux" then "sha256-APuO9CfU0G3KvZD1GWJm4HcxrfKnRmlkzm9PbR6MvBE="
                       else if system == "aarch64-linux" then "sha256-JUAsTLy32i0zfLFoZjVCvJx1rpsQ0tJwj1KJ6MqdGQI="
                       else if system == "x86_64-darwin" then "sha256-uiXuhXp2+o9MECL23/QHLJoBAHRT1nxhz7qhqhwD4xc="
                       else if system == "aarch64-darwin" then "sha256-dveLZF4zjsfokynwrSJ5KZf0K9m16xRkOsMq3iJO62E="
                       else throw "Unsupported system: ${system}";

        rustDate = "2026-05-31";
        leanVersion = "v4.30.0-rc2";

        rustToolchainSha256 = if system == "x86_64-linux" then "sha256-tdLBvDewiNTUKOdMJ1pkU7mPrUY0xTFOZWdG9dDNiAk="
                              else if system == "aarch64-linux" then "sha256-5gGGsObb22cKc2beF5UWMEJN4Df4PM23hK0A4QJ/kEM="
                              else if system == "x86_64-darwin" then "sha256-v3By/ilhfQEfNECMoNlMC0pQndo5Lq1CTTbjcsaXMPw="
                              else if system == "aarch64-darwin" then "sha256-I8pM8VuoBc5R/4ZR3ZiuHmbQjn361QOXTTU+kD5B0p8="
                              else throw "Unsupported system: ${system}";

        leanToolchainSha256 = if system == "x86_64-linux" then "sha256-o47cQjSLK5YL8YZ2raaj+mGAvvO+dIDfVeP2L+WoyMs="
                              else if system == "aarch64-linux" then "sha256-IrEGcTEeI1q0/7tLtMiiKPcW05JvaU8kNY6y5eprYg4="
                              else if system == "x86_64-darwin" then "sha256-DDPmVkXjSLDr21LXcdvNkmGjD2v+sbUyY+REr3uylwI="
                              else if system == "aarch64-darwin" then "sha256-dpUCCLkhoGDKkDKPZxr7WrmkifxHi4MWLpD148z2vhg="
                              else throw "Unsupported system: ${system}";

        leantarPlatform = if system == "x86_64-linux" then "x86_64-unknown-linux-musl"
                          else if system == "aarch64-linux" then "aarch64-unknown-linux-musl"
                          else if system == "x86_64-darwin" then "x86_64-apple-darwin"
                          else if system == "aarch64-darwin" then "aarch64-apple-darwin"
                          else throw "Unsupported system: ${system}";

        leantarSha256 = if system == "x86_64-linux" then "sha256-LLxAyiFCJ6DlNnIcAhutcZqALdhrHy2JiVce+vv709E="
                        else if system == "aarch64-linux" then "sha256-Jut3VDaIPj1c2tJ681ucNyEscxBjFoY+ofxfjsLMneQ="
                        else if system == "x86_64-darwin" then "sha256-58eNYGxlMHhiuw/sWqRG1ves4TN7HkiVzEfZH3VlmWw="
                        else if system == "aarch64-darwin" then "sha256-tbWQ0vhC4jWZPsdW09vWCKE8iP1U02p7K2WjY7LuXjU="
                        else throw "Unsupported system: ${system}";

        mathlibCacheDownloadSha256 = if system == "x86_64-linux" then "sha256-n67tKjzZm5LsDU1Dl9kaOFKrQw+8YE201F0toYu1C3s="
                                     else if system == "aarch64-linux" then "sha256-9Yj5BAv6V5BTLd/nOWzIuqTDJPKwqR28bg7m9+46K98="
                                     else if system == "x86_64-darwin" then "sha256-DBdUmPfheeLTVwaVUzkB541Y9CWSQN6gmxBnJ3oxL4c="
                                     else if system == "aarch64-darwin" then "sha256-wv2NZcKiyYaW6L/o7+oHWZdYZhVYLzZjyQczoaHRJnk="
                                     else throw "Unsupported system: ${system}";

        linuxDynamicLinker = if system == "x86_64-linux" then "/lib64/ld-linux-x86-64.so.2"
                             else if system == "aarch64-linux" then "/lib/ld-linux-aarch64.so.1"
                             else "";

        linuxFhsEnv = if pkgs.stdenv.isLinux then pkgs.buildFHSEnv {
          name = "anneal-linux-fhs";
          targetPkgs = pkgs: with pkgs; [
            stdenv.cc.cc
            zlib
            gmp
            libffi
            ncurses
            openssl
          ];
          runScript = "bash";
        } else null;

        runLeanCommand = command:
          if pkgs.stdenv.isLinux
          then "${linuxFhsEnv}/bin/anneal-linux-fhs -c ${pkgs.lib.escapeShellArg command}"
          else command;

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
            pname = "${pname}-${system}";
            inherit version sha256 buildPhase;

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

        fetchLeantar = { version, sha256 }:
          let
            archive = pkgs.fetchurl {
              name = "leantar-${version}-${leantarPlatform}.tar.gz";
              url = "https://github.com/digama0/leangz/releases/download/v${version}/leantar-v${version}-${leantarPlatform}.tar.gz";
              inherit sha256;
            };
          in
          pkgs.stdenv.mkDerivation {
            pname = "leantar-${system}";
            inherit version;

            src = archive;
            dontPatchShebangs = true;
            dontPatchELF = true;
            dontStrip = true;

            nativeBuildInputs = with pkgs; [
              gnutar
              gzip
            ];

            unpackPhase = ''
              runHook preUnpack
              tar -xzf "$src" --strip-components=1
              runHook postUnpack
            '';

            installPhase = ''
              runHook preInstall
              mkdir -p "$out/bin"
              cp leantar "$out/bin/leantar"
              chmod +x "$out/bin/leantar"
              runHook postInstall
            '';
          };
      in
      {
        packages.aeneas-download = fetchAeneas {
          target = aeneasTarget;
          releaseTag = "nightly-2026.06.03";
          sha256 = aeneasSha256;
        };

        # Extracts the toolchain metadata implied by the Aeneas archive.
        packages.aeneas-unpacked = pkgs.stdenv.mkDerivation {
          pname = "aeneas-unpacked";
          version = "1.0.0";

          src = self.packages.${system}.aeneas-download;

          nativeBuildInputs = with pkgs; [
            gnutar
            gzip
          ];

          dontUnpack = true;

          buildPhase = builtins.concatStringsSep "\n" [
            "mkdir -p $out"
            "tar -xzf $src -C $out"
            "chmod -R +w $out"
            "LEAN_RAW=\$(cat $out/backends/lean/lean-toolchain)"
            "LEAN_VERSION=\$(echo \"\$LEAN_RAW\" | sed -E 's|leanprover/lean4:v?||' | tr -d '\\n')"
            "if [ -z \"\$LEAN_VERSION\" ] || [ \"\$LEAN_VERSION\" = \"\$LEAN_RAW\" ]; then"
            "  echo \"ERROR: could not parse Lean toolchain from Aeneas archive: \$LEAN_RAW\" >&2"
            "  exit 1"
            "fi"
            "RUST_DATE=${rustDate}"
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

          src = self.packages.${system}.aeneas-download;

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
          pname = "mathlib-cache-download-${system}";
          version = "0.1.0";

          dontUnpack = true;

          # Preserve downloaded artifacts exactly.
          dontPatchShebangs = true;
          dontPatchELF = true;
          dontStrip = true;

          outputHashMode = "recursive";
          outputHashAlgo = "sha256";
          outputHash = mathlibCacheDownloadSha256;

          leanToolchainRaw = self.packages.${system}.lean-toolchain;
          metadataFiles = self.packages.${system}.aeneas-metadata-files;

          nativeBuildInputs = with pkgs; [
            git
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
            # `get-` downloads the linked .ltar files without also decompressing
            # them. That keeps this fixed-output derivation focused on network
            # materialization; the ordinary derivation below does decompression.
            (runLeanCommand "$leanToolchainRaw/bin/lake exe cache get-")
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
          leantar = self.packages.${system}.leantar;

          nativeBuildInputs = with pkgs; [
            gnutar
            zstd
          ];

          buildPhase = builtins.concatStringsSep "\n" [
            "mkdir -p $out/packages"
            "cp -r $mathlibCache/packages/* $out/packages/"
            "chmod -R +w $out/packages"
            # Lean v4.30.0-rc2's linux_aarch64 archive accidentally bundles an
            # x86_64 `leantar`, so do not rely on the Lean toolchain copy here.
            # Fetch the matching native `leantar` release directly instead.
            "LEANTAR_BIN=\"$leantar/bin/leantar\""
            "if [ ! -x \"\$LEANTAR_BIN\" ]; then"
            "  echo \"ERROR: leantar utility binary not found at \$LEANTAR_BIN!\""
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
            # In the final archive, every Lake dependency is vendored as a path
            # package. Seed those path packages with the build products that
            # `lake exe cache get-` originally unpacked into Lake's ordinary
            # project cache layout so the offline build does not need to rebuild
            # Mathlib or its dependencies from source.
            "mkdir -p ../../packages/mathlib/.lake"
            "cp -r $mathlibCache/.lake/build ../../packages/mathlib/.lake/"
            "if [ -d $mathlibCache/.lake/packages ]; then"
            "  for cached_pkg in $mathlibCache/.lake/packages/*; do"
            "    pkg_name=\$(basename \"\$cached_pkg\")"
            "    if [ -d \"\$cached_pkg/.lake\" ] && [ -d \"../../packages/\$pkg_name\" ]; then"
            "      mkdir -p \"../../packages/\$pkg_name/.lake\""
            "      cp -r \"\$cached_pkg/.lake/.\" \"../../packages/\$pkg_name/.lake/\""
            "    fi"
            "  done"
            "  chmod -R +w ../../packages"
            "fi"
            "python3 ${./rewrite-lake-vendor.py} --root . --packages-dir ../../packages"
            # Rewriting Git dependencies to final vendored path dependencies
            # changes Lake's dependency hashes even though the source content and
            # cached artifacts came from the same upstream revision. Run the
            # archive verification build in Lake's old mtime mode and make the
            # rewritten source/config inputs older than the already-unpacked
            # cache artifacts so Lake accepts the cache instead of deleting it
            # and trying to rebuild Mathlib from source in the Nix sandbox.
            "find . ../../packages -type f \\( -name \"*.lean\" -o -name \"lakefile.lean\" -o -name \"lakefile.toml\" -o -name \"lake-manifest.json\" -o -name \"lean-toolchain\" \\) -exec touch -h -d \"1970-01-01 00:00:00\" {} +"
            "test -f ../../packages/batteries/.lake/build/lib/lean/Batteries/Data/Array/Merge.olean"
            (runLeanCommand "lake --old build")
            "test -f ../../packages/batteries/.lake/build/lib/lean/Batteries/Data/Array/Merge.olean"
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
              import json
              import os
              import re
              import shutil

              import_re = re.compile(
                  r"^(?:public\s+)?(?:protected\s+)?(?:private\s+)?(?:meta\s+)?import\s+(.+)$"
              )
              mathlib_trace_re = re.compile(r"((?:Mathlib)(?:\.[A-Za-z0-9_']+)*)")

              def module_name_from_lean_path(pkg_dir, path):
                  rel = os.path.relpath(path, pkg_dir)
                  if rel == "lakefile.lean" or not rel.endswith(".lean"):
                      return None
                  return os.path.splitext(rel)[0].replace(os.sep, ".")

              def imported_mathlib_modules(path):
                  imports = set()
                  with open(path, encoding="utf-8") as f:
                      for line in f:
                          match = import_re.match(line.strip())
                          if match is None:
                              continue
                          imports.update(
                              module
                              for module in match.group(1).split()
                              if module.startswith("Mathlib")
                          )
                  return imports

              def imported_mathlib_modules_from_trace(path):
                  try:
                      with open(path, encoding="utf-8") as f:
                          content = json.load(f)
                  except (OSError, json.JSONDecodeError):
                      return set()
                  return set(mathlib_trace_re.findall(json.dumps(content)))

              def collect_mathlib_closure(project_root, packages_root):
                  mathlib_dir = os.path.join(packages_root, "mathlib")
                  if not os.path.isdir(mathlib_dir):
                      return None

                  mathlib_imports = {}
                  seeds = set()
                  for scan_root in [project_root, packages_root]:
                      for root, dirs, files in os.walk(scan_root):
                          if ".git" in dirs:
                              dirs.remove(".git")
                          is_mathlib = root == mathlib_dir or root.startswith(mathlib_dir + os.sep)
                          for file in files:
                              path = os.path.join(root, file)
                              if file.endswith(".lean"):
                                  if is_mathlib:
                                      module_name = module_name_from_lean_path(mathlib_dir, path)
                                      if module_name is not None:
                                          mathlib_imports[module_name] = imported_mathlib_modules(path)
                                  else:
                                      seeds.update(imported_mathlib_modules(path))
                              elif file.endswith(".trace") and not is_mathlib:
                                  seeds.update(imported_mathlib_modules_from_trace(path))

                  closure = set()
                  stack = list(seeds)
                  while stack:
                      module_name = stack.pop()
                      if module_name in closure:
                          continue
                      closure.add(module_name)
                      stack.extend(mathlib_imports.get(module_name, ()) - closure)

                  print(
                      "Mathlib reachability pruning will keep "
                      f"{len(closure)} modules seeded from {len(seeds)} direct imports."
                  )
                  return closure

              def prune_package(pkg_dir, mathlib_closure):
                  print(f"Pruning package at: {pkg_dir}")
                  traces_dir = os.path.join(pkg_dir, ".lake", "build", "lib", "lean")
                  if not os.path.exists(traces_dir):
                      print(f"No build traces found for {pkg_dir}, skipping pruning.")
                      return
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
                  if os.path.basename(pkg_dir) == "mathlib":
                      if mathlib_closure is None:
                          print("  - No Mathlib reachability closure found; keeping all modules.")
                          used_modules = set(os.path.splitext(rel)[0] for rel in all_lean_files)
                      else:
                          used_modules = set(module.replace(".", os.sep) for module in mathlib_closure)
                  else:
                      used_modules = set(os.path.splitext(rel)[0] for rel in all_lean_files)
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
              mathlib_closure = collect_mathlib_closure(os.path.abspath("."), packages_root)
              if os.path.exists(packages_root):
                  for name in os.listdir(packages_root):
                      pkg_dir = os.path.join(packages_root, name)
                      if os.path.isdir(pkg_dir):
                          prune_package(pkg_dir, mathlib_closure)

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
            gnutar
          ] ++ pkgs.lib.optionals stdenv.isLinux [
            patchelf
            file
          ];

          aeneasBuild = self.packages.${system}.aeneas-compiled;
          rustToolchain = self.packages.${system}.rust-toolchain;
          leanToolchain = self.packages.${system}.lean-toolchain;

          buildPhase = builtins.concatStringsSep "\n" ([
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
          ] ++ pkgs.lib.optionals pkgs.stdenv.isLinux [
            # Remove Nix dynamic-linker and RPATH references from ELF binaries.
            "echo \"Cleaning up Nix store references...\""
            "find $TMPDIR/dist_staging -type f -executable | while read -r file; do"
            "  if file \"\$file\" | grep -q \"ELF 64-bit\"; then"
            "    echo \"Patching and stripping \$file...\""
            "    if patchelf --print-interpreter \"\$file\" >/dev/null 2>&1; then"
            "      patchelf --set-interpreter ${linuxDynamicLinker} \"\$file\" || true"
            "    fi"
            "    patchelf --set-rpath \"\" \"\$file\" || true"
            "    strip \"\$file\" || true"
            "  fi"
            "done"
          ] ++ [
            "TRACE_ABS_RE='(^|[\"[:space:]=:])/(nix/store|build|private/tmp/nix-build|ANNEAL_PLACEHOLDER_ROOT)'"
            "if find $TMPDIR/dist_staging -type f -name \"*.trace\" -exec grep -EIl \"\$TRACE_ABS_RE\" {} + | tee /tmp/non-relocatable-staged-traces | grep -q .; then"
            "  echo \"ERROR: non-relocatable paths remain in staged Lake trace files\" >&2"
            "  cat /tmp/non-relocatable-staged-traces >&2"
            "  exit 1"
            "fi"
            "chmod -R a-w $TMPDIR/dist_staging"
            "cd $TMPDIR/dist_staging"
            "tar -cf $out *"
          ]);
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

        # CI caches this archive across runs, so use a moderate compression
        # level that keeps cache/artifact size under control without making
        # from-scratch PR rebuilds pay the level-19 CPU cost.
        packages.omnibus-archive-ci = self.packages.${system}.omnibus-archive.overrideAttrs (_: {
          ANNEAL_ZSTD_LEVEL = 6;
        });

        packages.rust-toolchain = fetchRustToolchain {
          inherit rustDate;
          sha256 = rustToolchainSha256;
        };

        packages.lean-toolchain = fetchLeanToolchain {
          inherit leanVersion;
          sha256 = leanToolchainSha256;
        };

        packages.leantar = fetchLeantar {
          version = "0.1.16";
          sha256 = leantarSha256;
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
