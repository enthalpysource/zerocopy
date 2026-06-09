#!/usr/bin/env python3
#
# Copyright 2026 The Fuchsia Authors
#
# Licensed under a BSD-style license <LICENSE-BSD>, Apache License, Version 2.0
# <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>, or the MIT
# license <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.
# This file may not be copied, modified, or distributed except according to
# those terms.

"""Write semantic Anneal CI metrics which are not exposed by the Actions jobs API."""

from __future__ import annotations

import argparse
import json
import os
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


SEMANTIC_ARTIFACT_KIND = "anneal-ci-semantic-metrics"


def optional_int(value: str | None) -> int | None:
    if value in (None, ""):
        return None
    return int(value)


def normalize_cache_hit(value: str | None, *, step_ran: bool) -> str | None:
    if value in (None, ""):
        return "false" if step_ran else None
    lower = value.lower()
    if lower in ("true", "false"):
        return lower
    return value


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--archive-size-bytes")
    parser.add_argument("--release-asset-limit-bytes", default="2147483647")
    parser.add_argument("--main-nix-cache-hit")
    parser.add_argument("--pr-nix-cache-hit")
    parser.add_argument("--event", default=os.environ.get("GITHUB_EVENT_NAME"))
    parser.add_argument("--ref", default=os.environ.get("GITHUB_REF"))
    parser.add_argument("--sha", default=os.environ.get("GITHUB_SHA"))
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    archive_size = optional_int(args.archive_size_bytes)
    release_limit = optional_int(args.release_asset_limit_bytes)

    metrics: list[dict[str, Any]] = []
    if archive_size is not None:
        metrics.append(
            {
                "metric_id": "archive.compressed_bytes",
                "value": archive_size,
                "unit": "bytes",
                "scope": "archive",
            }
        )
    if archive_size is not None and release_limit is not None:
        metrics.append(
            {
                "metric_id": "archive.release_limit_headroom_bytes",
                "value": release_limit - archive_size,
                "unit": "bytes",
                "scope": "archive",
            }
        )

    payload = {
        "schema_version": 1,
        "kind": SEMANTIC_ARTIFACT_KIND,
        "generated_at": datetime.now(timezone.utc).isoformat().replace("+00:00", "Z"),
        "archive_size_bytes": archive_size,
        "release_asset_limit_bytes": release_limit,
        "main_nix_cache_hit": normalize_cache_hit(args.main_nix_cache_hit, step_ran=True),
        "pr_nix_cache_hit": normalize_cache_hit(args.pr_nix_cache_hit, step_ran=args.event == "pull_request"),
        "event": args.event,
        "ref": args.ref,
        "sha": args.sha,
        "metrics": metrics,
    }

    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
