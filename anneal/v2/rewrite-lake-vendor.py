#!/usr/bin/env python3
"""Rewrite Lake dependencies to local vendored paths."""

from __future__ import annotations

import argparse
import json
import os
import re
from pathlib import Path


REMOTE_TOML_KEYS = {"git", "url", "rev", "scope", "subDir", "inputRev"}


def quote_name(name: str) -> str:
    if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_']*", name):
        return name
    escaped = name.replace("\\", "\\\\").replace('"', '\\"')
    return f'"{escaped}"'


def rel(from_file: Path, to_dir: Path) -> str:
    return os.path.relpath(to_dir, from_file.parent)


def package_dirs(packages_dir: Path) -> dict[str, Path]:
    return {
        child.name: child
        for child in packages_dir.iterdir()
        if child.is_dir() and child.name != ".lake"
    }


def replace_require_toml_blocks(path: Path, packages: dict[str, Path]) -> bool:
    content = path.read_text()
    block_re = re.compile(r"(?ms)^\s*\[\[require\]\]\s*\n(?:(?!^\s*\[\[).*\n?)*")

    changed = False

    def replace(match: re.Match[str]) -> str:
        nonlocal changed
        block = match.group(0)
        name_match = re.search(r'(?m)^\s*name\s*=\s*"([^"]+)"\s*$', block)
        if not name_match:
            return block
        name = name_match.group(1)
        dep_dir = packages.get(name)
        if dep_dir is None:
            return block
        if re.search(r'(?m)^\s*path\s*=', block) and not any(k in block for k in REMOTE_TOML_KEYS):
            return block

        changed = True
        return f'[[require]]\nname = "{name}"\npath = "{rel(path, dep_dir)}"\n'

    new_content = block_re.sub(replace, content)
    if changed:
        path.write_text(new_content)
    return changed


def replace_require_lean(path: Path, packages: dict[str, Path]) -> bool:
    content = path.read_text()
    changed = False

    def local_req(name: str) -> str | None:
        dep_dir = packages.get(name)
        if dep_dir is None:
            return None
        return f'require {quote_name(name)} from "{rel(path, dep_dir)}"'

    patterns = [
        re.compile(
            r'require\s+([A-Za-z_][A-Za-z0-9_\']*|«([^»]+)»|"([^"]+)")\s+from\s+git\s*\n\s*"[^"]+"\s*@\s*"[^"]+"'
        ),
        re.compile(
            r'require\s+([A-Za-z_][A-Za-z0-9_\']*|«([^»]+)»|"([^"]+)")\s+from\s+git\s+"[^"]+"\s*@\s*"[^"]+"'
        ),
        re.compile(r'require\s+"[^"]+"\s*/\s*"([^"]+)"\s*@\s*git\s*"[^"]+"'),
    ]

    for pattern in patterns:

        def replace(match: re.Match[str]) -> str:
            nonlocal changed
            name = next(group for group in match.groups() if group)
            replacement = local_req(name)
            if replacement is None:
                return match.group(0)
            changed = True
            return replacement

        content = pattern.sub(replace, content)

    if changed:
        path.write_text(content)
    return changed


def rewrite_manifest(path: Path, packages: dict[str, Path]) -> bool:
    data = json.loads(path.read_text())
    changed = False
    rewritten = []

    for pkg in data.get("packages", []):
        name = pkg.get("name")
        dep_dir = packages.get(name)
        if dep_dir is None:
            rewritten.append(pkg)
            continue

        new_pkg = {
            "type": "path",
            "name": name,
            "dir": rel(path, dep_dir),
            "inherited": pkg.get("inherited", False),
        }
        if pkg != new_pkg:
            changed = True
        rewritten.append(new_pkg)

    if changed:
        data["packages"] = rewritten
        path.write_text(json.dumps(data, indent=2) + "\n")
    return changed


def rewrite_trace_prefixes(root: Path, packages: dict[str, Path]) -> int:
    prefixes = [(root.resolve(), ""), *((p.resolve(), "") for p in packages.values())]
    count = 0

    for trace in [*root.rglob("*.trace"), *(t for p in packages.values() for t in p.rglob("*.trace"))]:
        try:
            content = trace.read_text()
        except UnicodeDecodeError:
            continue
        new_content = content
        for prefix, replacement in prefixes:
            new_content = new_content.replace(str(prefix) + os.sep, replacement)
        if new_content != content:
            trace.write_text(new_content)
            count += 1
    return count


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True, help="Root Lake package directory")
    parser.add_argument("--packages-dir", type=Path, required=True, help="Vendored packages directory")
    parser.add_argument(
        "--rewrite-traces",
        action="store_true",
        help="Also strip package/root absolute prefixes from .trace files",
    )
    args = parser.parse_args()

    root = args.root.resolve()
    packages_dir = args.packages_dir.resolve()
    packages = package_dirs(packages_dir)

    changed = 0
    for lakefile in [*root.rglob("lakefile.lean"), *packages_dir.rglob("lakefile.lean")]:
        changed += int(replace_require_lean(lakefile, packages))
    for lakefile in [*root.rglob("lakefile.toml"), *packages_dir.rglob("lakefile.toml")]:
        changed += int(replace_require_toml_blocks(lakefile, packages))
    for manifest in [*root.rglob("lake-manifest.json"), *packages_dir.rglob("lake-manifest.json")]:
        changed += int(rewrite_manifest(manifest, packages))

    trace_count = rewrite_trace_prefixes(root, packages) if args.rewrite_traces else 0
    print(f"rewrote {changed} Lake metadata files and {trace_count} trace files")


if __name__ == "__main__":
    main()
