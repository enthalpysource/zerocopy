#!/usr/bin/env python3
#
# Copyright 2026 The Fuchsia Authors
#
# Licensed under a BSD-style license <LICENSE-BSD>, Apache License, Version 2.0
# <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>, or the MIT
# license <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.
# This file may not be copied, modified, or distributed except according to
# those terms.

"""Unit tests for writing Anneal CI semantic metrics artifacts."""

from __future__ import annotations

import unittest

from . import write_semantic_metrics


class WriteSemanticMetricsTests(unittest.TestCase):
    def test_normalizes_known_empty_cache_misses(self) -> None:
        self.assertEqual(write_semantic_metrics.normalize_cache_hit("", step_ran=True), "false")
        self.assertEqual(write_semantic_metrics.normalize_cache_hit(None, step_ran=True), "false")

    def test_leaves_skipped_cache_restore_unknown(self) -> None:
        self.assertIsNone(write_semantic_metrics.normalize_cache_hit("", step_ran=False))
        self.assertIsNone(write_semantic_metrics.normalize_cache_hit(None, step_ran=False))

    def test_normalizes_action_output_case(self) -> None:
        self.assertEqual(write_semantic_metrics.normalize_cache_hit("TRUE", step_ran=True), "true")
        self.assertEqual(write_semantic_metrics.normalize_cache_hit("False", step_ran=True), "false")


if __name__ == "__main__":
    unittest.main()
