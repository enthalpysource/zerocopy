#!/usr/bin/env python3
#
# Copyright 2026 The Fuchsia Authors
#
# Licensed under a BSD-style license <LICENSE-BSD>, Apache License, Version 2.0
# <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>, or the MIT
# license <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.
# This file may not be copied, modified, or distributed except according to
# those terms.

"""Unit tests for Anneal CI dashboard generation."""

from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from . import collect
from . import generate_dashboard


TESTDATA = Path(__file__).resolve().parent / "testdata"


class DashboardTests(unittest.TestCase):
    def test_generates_dashboard_assets(self) -> None:
        run = collect.read_json(TESTDATA / "run.json")
        jobs = collect.normalize_jobs_payload(collect.read_json(TESTDATA / "jobs.json"))
        raw_run = collect.collect_metrics(run, jobs, artifacts_dir=TESTDATA / "artifacts")

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            data_dir = root / "anneal-ci"
            out_dir = data_dir / "dashboard"
            collect.write_raw_run(data_dir, raw_run)
            collect.import_legacy_benchmark_action_data(TESTDATA / "legacy-data.js", data_dir)
            collect.rebuild_series(data_dir)

            payload = generate_dashboard.generate_dashboard(data_dir, out_dir)

            index = (out_dir / "index.html").read_text()
            data_js = (out_dir / "data.js").read_text()
            self.assertIn("Anneal CI Metrics", index)
            self.assertIn("window.ANNEAL_CI_METRICS", data_js)
            self.assertIn("workflow.wall_seconds", payload["series"])
            self.assertEqual(payload["latest"]["run"]["run_id"], 123)


if __name__ == "__main__":
    unittest.main()
