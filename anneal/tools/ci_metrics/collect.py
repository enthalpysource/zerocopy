#!/usr/bin/env python3
#
# Copyright 2026 The Fuchsia Authors
#
# Licensed under a BSD-style license <LICENSE-BSD>, Apache License, Version 2.0
# <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>, or the MIT
# license <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.
# This file may not be copied, modified, or distributed except according to
# those terms.

"""Collect structured metrics for one completed Anneal GitHub Actions run."""

from __future__ import annotations

import argparse
import json
import math
import os
import re
import statistics
import sys
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


DEFAULT_REGISTRY = Path(__file__).resolve().parent / "registry.json"
SEMANTIC_ARTIFACT_KIND = "anneal-ci-semantic-metrics"
GITHUB_API = "https://api.github.com"


JOB_GROUP_NAMES = {
    "Anneal Static Checks": "static_checks",
    "Build Anneal Toolchain Archive": "nix_archive",
    "Anneal Tests": "anneal_tests",
    "Run V2 tests": "v2",
    "All checks succeeded (anneal.yml)": "all_checks",
}


LEGACY_METRIC_IDS = {
    ("Docker Image Size", "Docker Image Size"): "legacy.docker_image_size.mb",
    ("Docker Build Time", "Docker Build Time"): "legacy.docker_build_time.seconds",
    ("CI Durations", "Docker Pull Time"): "legacy.docker_pull_time.seconds",
    ("CI Durations", "Test Time"): "legacy.test_time.seconds",
    ("CI Durations", "Total CI Duration (All Steps)"): "legacy.anneal_tests_job_duration.seconds",
}


def read_json(path: Path) -> Any:
    with path.open() as f:
        return json.load(f)


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")


def parse_github_time(value: str | None) -> datetime | None:
    if not value:
        return None
    return datetime.fromisoformat(value.replace("Z", "+00:00")).astimezone(timezone.utc)


def seconds_between(start: str | None, end: str | None) -> int | None:
    start_time = parse_github_time(start)
    end_time = parse_github_time(end)
    if start_time is None or end_time is None:
        return None
    return max(0, int(round((end_time - start_time).total_seconds())))


def isoformat_utc(value: datetime | None) -> str | None:
    if value is None:
        return None
    return value.astimezone(timezone.utc).isoformat().replace("+00:00", "Z")


def slug(value: str) -> str:
    value = value.lower()
    value = re.sub(r"[^a-z0-9]+", "_", value)
    return value.strip("_") or "unnamed"


def percentile(values: list[int], pct: float) -> int | None:
    if not values:
        return None
    sorted_values = sorted(values)
    index = max(0, min(len(sorted_values) - 1, math.ceil((pct / 100.0) * len(sorted_values)) - 1))
    return sorted_values[index]


def normalize_jobs_payload(payload: Any) -> list[dict[str, Any]]:
    if isinstance(payload, dict) and isinstance(payload.get("jobs"), list):
        return payload["jobs"]
    if isinstance(payload, list):
        return payload
    raise ValueError("jobs JSON must be either a list or an object with a jobs list")


def github_api_json(path: str, token: str | None) -> Any:
    request = urllib.request.Request(f"{GITHUB_API}{path}")
    request.add_header("Accept", "application/vnd.github+json")
    request.add_header("X-GitHub-Api-Version", "2022-11-28")
    if token:
        request.add_header("Authorization", f"Bearer {token}")

    try:
        with urllib.request.urlopen(request) as response:
            return json.loads(response.read().decode("utf-8"))
    except urllib.error.HTTPError as error:
        body = error.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"GitHub API request failed for {path}: HTTP {error.code}: {body}") from error


def fetch_workflow_run(repo: str, run_id: int, token: str | None) -> dict[str, Any]:
    payload = github_api_json(f"/repos/{repo}/actions/runs/{run_id}", token)
    if not isinstance(payload, dict):
        raise ValueError("workflow run API returned a non-object payload")
    return payload


def fetch_workflow_jobs(repo: str, run_id: int, token: str | None) -> list[dict[str, Any]]:
    jobs: list[dict[str, Any]] = []
    page = 1
    while True:
        payload = github_api_json(f"/repos/{repo}/actions/runs/{run_id}/jobs?per_page=100&page={page}", token)
        page_jobs = normalize_jobs_payload(payload)
        jobs.extend(page_jobs)
        if len(page_jobs) < 100:
            return jobs
        page += 1


def load_registry(path: Path) -> dict[str, Any]:
    registry = read_json(path)
    if registry.get("schema_version") != 1:
        raise ValueError(f"unsupported registry schema in {path}")
    return registry


def run_context(run: dict[str, Any]) -> dict[str, Any]:
    repository = run.get("repository") or {}
    return {
        "run_id": run.get("id"),
        "run_attempt": run.get("run_attempt"),
        "run_number": run.get("run_number"),
        "workflow_id": run.get("workflow_id"),
        "workflow_name": run.get("name") or run.get("workflow_name"),
        "event": run.get("event"),
        "head_branch": run.get("head_branch"),
        "head_sha": run.get("head_sha"),
        "display_title": run.get("display_title"),
        "status": run.get("status"),
        "conclusion": run.get("conclusion"),
        "created_at": run.get("created_at"),
        "run_started_at": run.get("run_started_at"),
        "updated_at": run.get("updated_at"),
        "html_url": run.get("html_url"),
        "repository": repository.get("full_name") or run.get("repository_full_name"),
        "epoch": "nix-archive",
    }


def metric_point(
    context: dict[str, Any],
    metric_id: str,
    value: int | float,
    *,
    source: str,
    scope: str,
    unit: str | None = None,
    job: dict[str, Any] | None = None,
    step: dict[str, Any] | None = None,
    tags: dict[str, Any] | None = None,
    extra: dict[str, Any] | None = None,
) -> dict[str, Any]:
    point = {
        "metric_id": metric_id,
        "value": value,
        "unit": unit,
        "source": source,
        "scope": scope,
        "run": context,
        "tags": tags or {},
    }
    if job is not None:
        point["job"] = {
            "id": job.get("id"),
            "name": job.get("name"),
            "conclusion": job.get("conclusion"),
            "started_at": job.get("started_at"),
            "completed_at": job.get("completed_at"),
            "html_url": job.get("html_url"),
            "runner_name": job.get("runner_name"),
            "runner_group_name": job.get("runner_group_name"),
            "labels": job.get("labels") or [],
        }
    if step is not None:
        point["step"] = {
            "number": step.get("number"),
            "name": step.get("name"),
            "conclusion": step.get("conclusion"),
            "started_at": step.get("started_at"),
            "completed_at": step.get("completed_at"),
        }
    if extra:
        point.update(extra)
    return point


def job_group(job: dict[str, Any]) -> tuple[str | None, dict[str, str]]:
    name = str(job.get("name") or "")
    if name in JOB_GROUP_NAMES:
        return JOB_GROUP_NAMES[name], {}

    match = re.fullmatch(r"Verify example \((?P<example>[^)]+)\)", name)
    if match:
        example = match.group("example")
        return "verify_examples", {"example": example}

    return None, {}


def infer_needs(jobs: list[dict[str, Any]]) -> dict[int, list[int]]:
    by_group: dict[str, list[int]] = {}
    verify_ids: list[int] = []
    for job in jobs:
        job_id = job.get("id")
        if not isinstance(job_id, int):
            continue
        group, _ = job_group(job)
        if group is None:
            continue
        by_group.setdefault(group, []).append(job_id)
        if group == "verify_examples":
            verify_ids.append(job_id)

    archive_ids = by_group.get("nix_archive", [])
    deps: dict[int, list[int]] = {}
    for group in ("anneal_tests", "verify_examples", "v2"):
        for job_id in by_group.get(group, []):
            deps[job_id] = list(archive_ids)

    all_check_deps = (
        by_group.get("static_checks", [])
        + archive_ids
        + by_group.get("anneal_tests", [])
        + by_group.get("v2", [])
        + verify_ids
    )
    for job_id in by_group.get("all_checks", []):
        deps[job_id] = all_check_deps
    return deps


def critical_path_seconds(jobs: list[dict[str, Any]]) -> int | None:
    durations = {
        job["id"]: seconds_between(job.get("started_at"), job.get("completed_at"))
        for job in jobs
        if isinstance(job.get("id"), int)
    }
    durations = {job_id: duration for job_id, duration in durations.items() if duration is not None}
    if not durations:
        return None

    deps = infer_needs(jobs)
    memo: dict[int, int] = {}

    def longest_to(job_id: int, active: set[int]) -> int:
        if job_id in memo:
            return memo[job_id]
        if job_id in active:
            raise ValueError(f"cycle in inferred Anneal job graph at job {job_id}")
        active.add(job_id)
        best_dep = 0
        for dep in deps.get(job_id, []):
            if dep in durations:
                best_dep = max(best_dep, longest_to(dep, active))
        active.remove(job_id)
        memo[job_id] = best_dep + durations[job_id]
        return memo[job_id]

    return max(longest_to(job_id, set()) for job_id in durations)


def workflow_span_seconds(jobs: list[dict[str, Any]]) -> int | None:
    starts = [parse_github_time(job.get("started_at")) for job in jobs]
    ends = [parse_github_time(job.get("completed_at")) for job in jobs]
    starts = [start for start in starts if start is not None]
    ends = [end for end in ends if end is not None]
    if not starts or not ends:
        return None
    return max(0, int(round((max(ends) - min(starts)).total_seconds())))


def completed_job_duration(job: dict[str, Any]) -> int | None:
    if job.get("status") != "completed":
        return None
    return seconds_between(job.get("started_at"), job.get("completed_at"))


def collect_job_metrics(context: dict[str, Any], jobs: list[dict[str, Any]]) -> list[dict[str, Any]]:
    points: list[dict[str, Any]] = []
    verify_durations: list[int] = []

    for job in jobs:
        group, tags = job_group(job)
        duration = completed_job_duration(job)
        if group is None or duration is None:
            continue

        job_tags = dict(tags)
        if job.get("runner_name"):
            job_tags["runner_name"] = job.get("runner_name")
        if job.get("runner_group_name"):
            job_tags["runner_group_name"] = job.get("runner_group_name")

        if group == "verify_examples":
            verify_durations.append(duration)
            metric_id = f"job.verify_examples.{slug(tags.get('example', 'unknown'))}.seconds"
        elif group == "all_checks":
            metric_id = "job.all_checks.seconds"
        else:
            metric_id = f"job.{group}.seconds"

        points.append(
            metric_point(
                context,
                metric_id,
                duration,
                source="github_actions_job",
                scope="job",
                unit="seconds",
                job=job,
                tags=job_tags,
            )
        )

    if verify_durations:
        aggregates = {
            "job.verify_examples.max_seconds": max(verify_durations),
            "job.verify_examples.p50_seconds": int(round(statistics.median(verify_durations))),
            "job.verify_examples.p90_seconds": percentile(verify_durations, 90),
            "job.verify_examples.runner_seconds": sum(verify_durations),
        }
        for metric_id, value in aggregates.items():
            if value is not None:
                points.append(
                    metric_point(
                        context,
                        metric_id,
                        value,
                        source="computed",
                        scope="job_group",
                        unit="seconds",
                        tags={"job_group": "verify_examples"},
                    )
                )

    return points


def collect_step_metrics(context: dict[str, Any], jobs: list[dict[str, Any]]) -> list[dict[str, Any]]:
    points: list[dict[str, Any]] = []
    for job in jobs:
        group, tags = job_group(job)
        if group is None or group == "all_checks":
            continue
        for step in job.get("steps") or []:
            duration = seconds_between(step.get("started_at"), step.get("completed_at"))
            if duration is None:
                continue
            name = str(step.get("name") or "")
            if name in ("Set up job", "Complete job"):
                continue
            step_tags = dict(tags)
            if group == "verify_examples":
                example_slug = slug(step_tags.get("example", "unknown"))
                metric_id = f"step.verify_examples.{example_slug}.{slug(name)}.seconds"
            else:
                metric_id = f"step.{group}.{slug(name)}.seconds"
            points.append(
                metric_point(
                    context,
                    metric_id,
                    duration,
                    source="github_actions_step",
                    scope="step",
                    unit="seconds",
                    job=job,
                    step=step,
                    tags=step_tags,
                )
            )
    return points


def collect_workflow_metrics(context: dict[str, Any], run: dict[str, Any], jobs: list[dict[str, Any]]) -> list[dict[str, Any]]:
    values = {
        "workflow.wall_seconds": workflow_span_seconds(jobs),
        "workflow.queue_seconds": seconds_between(run.get("created_at"), run.get("run_started_at")),
        "workflow.total_seconds": seconds_between(run.get("created_at"), run.get("updated_at")),
        "workflow.runner_seconds": sum(duration for job in jobs if (duration := completed_job_duration(job)) is not None),
        "workflow.critical_path_seconds": critical_path_seconds(jobs),
    }
    points = []
    for metric_id, value in values.items():
        if value is None:
            continue
        points.append(metric_point(context, metric_id, value, source="computed", scope="workflow", unit="seconds"))
    return points


def parse_optional_int(value: Any) -> int | None:
    if value in (None, ""):
        return None
    return int(value)


def parse_cache_hit(value: Any) -> int | None:
    if value in (None, ""):
        return None
    if isinstance(value, bool):
        return 1 if value else 0
    if str(value).lower() == "true":
        return 1
    if str(value).lower() == "false":
        return 0
    return None


def collect_semantic_artifact_metrics(context: dict[str, Any], artifacts_dir: Path | None) -> list[dict[str, Any]]:
    if artifacts_dir is None or not artifacts_dir.exists():
        return []

    points: list[dict[str, Any]] = []
    for path in sorted(artifacts_dir.rglob("*.json")):
        payload = read_json(path)
        if payload.get("kind") != SEMANTIC_ARTIFACT_KIND:
            continue

        emitted_metric_ids = set()
        for item in payload.get("metrics") or []:
            metric_id = item.get("metric_id")
            value = item.get("value")
            if not metric_id or value is None:
                continue
            emitted_metric_ids.add(metric_id)
            points.append(
                metric_point(
                    context,
                    metric_id,
                    value,
                    source="workflow_artifact",
                    scope=item.get("scope") or "semantic",
                    unit=item.get("unit"),
                    tags=item.get("tags") or {},
                    extra={"artifact_path": str(path)},
                )
            )

        archive_size = parse_optional_int(payload.get("archive_size_bytes"))
        release_limit = parse_optional_int(payload.get("release_asset_limit_bytes"))
        if archive_size is not None and "archive.compressed_bytes" not in emitted_metric_ids:
            points.append(
                metric_point(
                    context,
                    "archive.compressed_bytes",
                    archive_size,
                    source="workflow_artifact",
                    scope="archive",
                    unit="bytes",
                    extra={"artifact_path": str(path)},
                )
            )
        if (
            archive_size is not None
            and release_limit is not None
            and "archive.release_limit_headroom_bytes" not in emitted_metric_ids
        ):
            points.append(
                metric_point(
                    context,
                    "archive.release_limit_headroom_bytes",
                    release_limit - archive_size,
                    source="workflow_artifact",
                    scope="archive",
                    unit="bytes",
                    extra={"artifact_path": str(path)},
                )
            )

        main_cache_hit = parse_cache_hit(payload.get("main_nix_cache_hit"))
        if main_cache_hit is not None:
            points.append(
                metric_point(
                    context,
                    "nix.main_cache.hit",
                    main_cache_hit,
                    source="workflow_artifact",
                    scope="cache",
                    unit="boolean",
                    extra={"artifact_path": str(path)},
                )
            )

        pr_cache_hit = parse_cache_hit(payload.get("pr_nix_cache_hit"))
        if pr_cache_hit is not None:
            points.append(
                metric_point(
                    context,
                    "nix.pr_cache.hit",
                    pr_cache_hit,
                    source="workflow_artifact",
                    scope="cache",
                    unit="boolean",
                    extra={"artifact_path": str(path)},
                )
            )

    return points


def collect_metrics(
    run: dict[str, Any],
    jobs: list[dict[str, Any]],
    *,
    artifacts_dir: Path | None = None,
) -> dict[str, Any]:
    context = run_context(run)
    points: list[dict[str, Any]] = []
    points.extend(collect_workflow_metrics(context, run, jobs))
    points.extend(collect_job_metrics(context, jobs))
    points.extend(collect_step_metrics(context, jobs))
    points.extend(collect_semantic_artifact_metrics(context, artifacts_dir))

    completed_at = parse_github_time(run.get("updated_at")) or parse_github_time(run.get("run_started_at"))
    return {
        "schema_version": 1,
        "captured_at": isoformat_utc(datetime.now(timezone.utc)),
        "run": context,
        "completed_at": isoformat_utc(completed_at),
        "jobs": jobs,
        "metrics": points,
    }


def raw_run_path(output_dir: Path, raw_run: dict[str, Any]) -> Path:
    run = raw_run["run"]
    run_id = run.get("run_id")
    attempt = run.get("run_attempt") or 1
    if run_id is None:
        raise ValueError("run ID is missing")
    return output_dir / "raw" / "runs" / f"{run_id}-attempt-{attempt}.json"


def write_raw_run(output_dir: Path, raw_run: dict[str, Any]) -> Path:
    path = raw_run_path(output_dir, raw_run)
    write_json(path, raw_run)
    return path


def series_sort_key(point: dict[str, Any]) -> tuple[str, int, int]:
    run = point.get("run") or {}
    started_at = run.get("run_started_at") or run.get("created_at") or ""
    return (started_at, int(run.get("run_id") or 0), int(run.get("run_attempt") or 0))


def rebuild_series(output_dir: Path) -> None:
    series_dir = output_dir / "series"
    series_dir.mkdir(parents=True, exist_ok=True)
    for existing in series_dir.glob("*.json"):
        existing.unlink()

    by_metric: dict[str, list[dict[str, Any]]] = {}
    raw_runs = [read_json(raw_path) for raw_path in sorted((output_dir / "raw" / "runs").glob("*.json"))]
    raw_runs.sort(
        key=lambda raw: (
            (raw.get("run") or {}).get("run_started_at") or (raw.get("run") or {}).get("created_at") or "",
            int((raw.get("run") or {}).get("run_id") or 0),
            int((raw.get("run") or {}).get("run_attempt") or 0),
        )
    )
    latest_run = raw_runs[-1] if raw_runs else None
    for raw in raw_runs:
        for point in raw.get("metrics") or []:
            by_metric.setdefault(point["metric_id"], []).append(point)

    manifest_metrics = []
    for metric_id, points in sorted(by_metric.items()):
        points = sorted(points, key=series_sort_key)
        write_json(
            series_dir / f"{metric_id}.json",
            {
                "schema_version": 1,
                "metric_id": metric_id,
                "points": points,
            },
        )
        manifest_metrics.append(
            {
                "metric_id": metric_id,
                "points": len(points),
                "latest_value": points[-1].get("value") if points else None,
                "latest_run_id": (points[-1].get("run") or {}).get("run_id") if points else None,
            }
        )

    write_json(
        output_dir / "manifest.json",
        {
            "schema_version": 1,
            "generated_at": isoformat_utc(datetime.now(timezone.utc)),
            "metric_count": len(manifest_metrics),
            "metrics": manifest_metrics,
        },
    )
    if latest_run is not None:
        write_json(output_dir / "latest.json", latest_run)


def parse_benchmark_action_data_js(path: Path) -> dict[str, Any]:
    text = path.read_text()
    prefix = "window.BENCHMARK_DATA = "
    if text.startswith(prefix):
        text = text[len(prefix) :]
    return json.loads(text)


def import_legacy_benchmark_action_data(data_js_path: Path | None, output_dir: Path) -> Path | None:
    if data_js_path is None or not data_js_path.is_file():
        return None

    data = parse_benchmark_action_data_js(data_js_path)
    legacy_points: list[dict[str, Any]] = []
    for suite_name, entries in (data.get("entries") or {}).items():
        for entry in entries:
            for bench in entry.get("benches") or []:
                metric_id = LEGACY_METRIC_IDS.get((suite_name, bench.get("name")))
                if metric_id is None:
                    continue
                commit = entry.get("commit") or {}
                legacy_points.append(
                    {
                        "metric_id": metric_id,
                        "value": bench.get("value"),
                        "unit": bench.get("unit"),
                        "source": "github_action_benchmark",
                        "suite": suite_name,
                        "bench_name": bench.get("name"),
                        "commit": commit,
                        "date": commit.get("date"),
                        "tags": {"epoch": "docker-ci" if "docker" in metric_id else "legacy"},
                    }
                )

    legacy = {
        "schema_version": 1,
        "generated_at": isoformat_utc(datetime.now(timezone.utc)),
        "source": str(data_js_path),
        "points": legacy_points,
    }
    out = output_dir / "legacy" / "benchmark-action.json"
    write_json(out, legacy)
    return out


def load_inputs(args: argparse.Namespace) -> tuple[dict[str, Any], list[dict[str, Any]]]:
    if args.run_json:
        run = read_json(args.run_json)
    else:
        token = os.environ.get(args.token_env)
        run = fetch_workflow_run(args.repo, args.run_id, token)

    if args.jobs_json:
        jobs = normalize_jobs_payload(read_json(args.jobs_json))
    else:
        token = os.environ.get(args.token_env)
        jobs = fetch_workflow_jobs(args.repo, args.run_id, token)

    return run, jobs


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    default_run_id = os.environ.get("GITHUB_RUN_ID")
    parser.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", "google/zerocopy"))
    parser.add_argument("--run-id", type=int, default=int(default_run_id) if default_run_id else None)
    parser.add_argument("--run-json", type=Path)
    parser.add_argument("--jobs-json", type=Path)
    parser.add_argument("--artifacts-dir", type=Path)
    parser.add_argument("--registry", type=Path, default=DEFAULT_REGISTRY)
    parser.add_argument("--output-dir", required=True, type=Path)
    parser.add_argument("--legacy-data-js", type=Path)
    parser.add_argument("--token-env", default="GITHUB_TOKEN")
    args = parser.parse_args()
    if args.run_json is None and args.run_id is None:
        parser.error("--run-id is required unless --run-json is provided")
    return args


def main() -> None:
    args = parse_args()
    load_registry(args.registry)
    run, jobs = load_inputs(args)
    raw_run = collect_metrics(run, jobs, artifacts_dir=args.artifacts_dir)
    raw_path = write_raw_run(args.output_dir, raw_run)
    import_legacy_benchmark_action_data(args.legacy_data_js, args.output_dir)
    rebuild_series(args.output_dir)
    print(f"Wrote {raw_path}", file=sys.stderr)
    print(f"Collected {len(raw_run['metrics'])} metric points", file=sys.stderr)


if __name__ == "__main__":
    main()
