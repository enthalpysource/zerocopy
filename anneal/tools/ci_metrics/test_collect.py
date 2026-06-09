#!/usr/bin/env python3
#
# Copyright 2026 The Fuchsia Authors
#
# Licensed under a BSD-style license <LICENSE-BSD>, Apache License, Version 2.0
# <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>, or the MIT
# license <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.
# This file may not be copied, modified, or distributed except according to
# those terms.

"""Unit tests for Anneal CI metrics collection."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from . import collect


TESTDATA = Path(__file__).resolve().parent / "testdata"


def metric_values(raw_run: dict) -> dict[str, list[int | float]]:
    values: dict[str, list[int | float]] = {}
    for point in raw_run["metrics"]:
        values.setdefault(point["metric_id"], []).append(point["value"])
    return values


class CollectTests(unittest.TestCase):
    def test_collects_workflow_job_step_and_semantic_metrics(self) -> None:
        run = collect.read_json(TESTDATA / "run.json")
        jobs = collect.normalize_jobs_payload(collect.read_json(TESTDATA / "jobs.json"))

        raw_run = collect.collect_metrics(run, jobs, artifacts_dir=TESTDATA / "artifacts")
        values = metric_values(raw_run)

        self.assertEqual(values["workflow.wall_seconds"], [542])
        self.assertEqual(values["workflow.queue_seconds"], [60])
        self.assertEqual(values["workflow.total_seconds"], [602])
        self.assertEqual(values["workflow.runner_seconds"], [1357])
        self.assertEqual(values["workflow.critical_path_seconds"], [537])
        self.assertEqual(values["job.nix_archive.seconds"], [60])
        self.assertEqual(values["job.anneal_tests.seconds"], [475])
        self.assertEqual(values["job.verify_examples.max_seconds"], [300])
        self.assertEqual(values["job.verify_examples.p50_seconds"], [270])
        self.assertEqual(values["job.verify_examples.p90_seconds"], [300])
        self.assertEqual(values["step.anneal_tests.run_all_tests.seconds"], [280])
        self.assertEqual(values["archive.compressed_bytes"], [1000])
        self.assertEqual(values["archive.release_limit_headroom_bytes"], [2147482647])
        self.assertEqual(values["nix.main_cache.hit"], [1])

    def test_parses_cache_hits(self) -> None:
        self.assertEqual(collect.parse_cache_hit("true"), 1)
        self.assertEqual(collect.parse_cache_hit("false"), 0)
        self.assertIsNone(collect.parse_cache_hit(""))

    def test_writes_raw_run_rebuilds_series_and_imports_legacy_data(self) -> None:
        run = collect.read_json(TESTDATA / "run.json")
        jobs = collect.normalize_jobs_payload(collect.read_json(TESTDATA / "jobs.json"))
        raw_run = collect.collect_metrics(run, jobs, artifacts_dir=TESTDATA / "artifacts")

        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "anneal-ci"
            raw_path = collect.write_raw_run(out, raw_run)
            legacy_path = collect.import_legacy_benchmark_action_data(TESTDATA / "legacy-data.js", out)
            collect.rebuild_series(out)

            self.assertTrue(raw_path.is_file())
            self.assertTrue((out / "series" / "workflow.wall_seconds.json").is_file())
            self.assertTrue((out / "manifest.json").is_file())
            self.assertTrue((out / "latest.json").is_file())
            self.assertIsNotNone(legacy_path)

            legacy = collect.read_json(out / "legacy" / "benchmark-action.json")
            legacy_ids = {point["metric_id"] for point in legacy["points"]}
            self.assertEqual(
                legacy_ids,
                {
                    "legacy.docker_image_size.mb",
                    "legacy.docker_build_time.seconds",
                    "legacy.docker_pull_time.seconds",
                    "legacy.test_time.seconds",
                    "legacy.anneal_tests_job_duration.seconds",
                },
            )


if __name__ == "__main__":
    unittest.main()
