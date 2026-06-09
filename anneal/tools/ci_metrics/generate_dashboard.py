#!/usr/bin/env python3
#
# Copyright 2026 The Fuchsia Authors
#
# Licensed under a BSD-style license <LICENSE-BSD>, Apache License, Version 2.0
# <LICENSE-APACHE or https://www.apache.org/licenses/LICENSE-2.0>, or the MIT
# license <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your option.
# This file may not be copied, modified, or distributed except according to
# those terms.

"""Generate the static Anneal CI metrics dashboard."""

from __future__ import annotations

import argparse
import json
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


DEFAULT_REGISTRY = Path(__file__).resolve().parent / "registry.json"


def read_json(path: Path) -> Any:
    with path.open() as f:
        return json.load(f)


def write_json_js(path: Path, global_name: str, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(f"window.{global_name} = {json.dumps(value, indent=2, sort_keys=True)};\n")


def iso_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def load_series(data_dir: Path) -> dict[str, Any]:
    series: dict[str, Any] = {}
    for path in sorted((data_dir / "series").glob("*.json")):
        payload = read_json(path)
        series[payload["metric_id"]] = payload
    return series


def load_optional_json(path: Path) -> Any:
    if path.is_file():
        return read_json(path)
    return None


def dashboard_payload(data_dir: Path, registry_path: Path) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "generated_at": iso_now(),
        "registry": read_json(registry_path),
        "manifest": load_optional_json(data_dir / "manifest.json"),
        "latest": load_optional_json(data_dir / "latest.json"),
        "series": load_series(data_dir),
        "legacy": load_optional_json(data_dir / "legacy" / "benchmark-action.json"),
    }


INDEX_HTML = """<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Anneal CI Metrics</title>
  <style>
    :root {
      color-scheme: light dark;
      --bg: #f7f8fa;
      --panel: #ffffff;
      --ink: #172033;
      --muted: #5f6f89;
      --line: #d9e0ea;
      --blue: #1664c0;
      --green: #13795b;
      --red: #b42318;
      --amber: #8a5a00;
      --violet: #6f42c1;
      --shadow: 0 1px 2px rgba(17, 24, 39, 0.08);
    }
    @media (prefers-color-scheme: dark) {
      :root {
        --bg: #10141f;
        --panel: #171c2a;
        --ink: #edf2ff;
        --muted: #a7b1c5;
        --line: #2b3448;
        --blue: #7ab3ff;
        --green: #65c9a8;
        --red: #ff9b92;
        --amber: #f2c46d;
        --violet: #c3a5ff;
        --shadow: none;
      }
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: var(--bg);
      color: var(--ink);
      line-height: 1.45;
    }
    header {
      border-bottom: 1px solid var(--line);
      background: var(--panel);
    }
    .wrap {
      width: min(1180px, calc(100vw - 32px));
      margin: 0 auto;
    }
    .top {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
      padding: 20px 0 16px;
    }
    h1 {
      margin: 0;
      font-size: 28px;
      line-height: 1.1;
      letter-spacing: 0;
    }
    .meta {
      color: var(--muted);
      font-size: 13px;
      text-align: right;
    }
    nav {
      display: flex;
      gap: 8px;
      overflow-x: auto;
      padding-bottom: 12px;
    }
    button {
      border: 1px solid var(--line);
      border-radius: 8px;
      color: var(--ink);
      background: var(--panel);
      padding: 8px 12px;
      font: inherit;
      cursor: pointer;
      white-space: nowrap;
    }
    button.active {
      border-color: var(--blue);
      color: var(--blue);
      font-weight: 650;
    }
    main {
      padding: 24px 0 40px;
    }
    section[hidden] { display: none; }
    .grid {
      display: grid;
      gap: 12px;
    }
    .summary-grid {
      grid-template-columns: repeat(5, minmax(0, 1fr));
      margin-bottom: 18px;
    }
    .metric-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .panel {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 8px;
      box-shadow: var(--shadow);
      padding: 14px;
      min-width: 0;
    }
    .label {
      color: var(--muted);
      font-size: 12px;
      text-transform: uppercase;
      letter-spacing: 0.04em;
    }
    .value {
      margin-top: 6px;
      font-size: 26px;
      font-weight: 720;
      letter-spacing: 0;
    }
    .delta {
      margin-top: 4px;
      color: var(--muted);
      font-size: 13px;
    }
    .delta.bad { color: var(--red); }
    .delta.good { color: var(--green); }
    .panel h2 {
      margin: 0 0 8px;
      font-size: 18px;
      letter-spacing: 0;
    }
    .chart {
      width: 100%;
      height: 180px;
      display: block;
      border-top: 1px solid var(--line);
      margin-top: 10px;
      padding-top: 10px;
    }
    .chart text {
      fill: var(--muted);
      font-size: 11px;
    }
    .chart .axis { stroke: var(--line); }
    .chart .line { fill: none; stroke: var(--blue); stroke-width: 2.5; }
    .chart .dot { fill: var(--blue); }
    .timeline {
      position: relative;
      border-left: 1px solid var(--line);
      border-right: 1px solid var(--line);
      padding: 8px 0;
    }
    .bar-row {
      display: grid;
      grid-template-columns: 260px 1fr 80px;
      gap: 10px;
      align-items: center;
      min-height: 32px;
      font-size: 13px;
    }
    .bar-name {
      overflow: hidden;
      white-space: nowrap;
      text-overflow: ellipsis;
      color: var(--muted);
    }
    .bar-track {
      height: 18px;
      position: relative;
      background: color-mix(in srgb, var(--line) 60%, transparent);
      border-radius: 4px;
      overflow: hidden;
    }
    .bar {
      position: absolute;
      top: 0;
      bottom: 0;
      min-width: 2px;
      border-radius: 4px;
      background: var(--blue);
    }
    .bar.verify { background: var(--green); }
    .bar.archive { background: var(--violet); }
    .bar.static { background: var(--amber); }
    table {
      width: 100%;
      border-collapse: collapse;
      font-size: 13px;
    }
    th, td {
      text-align: left;
      border-bottom: 1px solid var(--line);
      padding: 8px 6px;
      vertical-align: top;
    }
    th { color: var(--muted); font-weight: 650; }
    .empty {
      color: var(--muted);
      padding: 22px;
      text-align: center;
    }
    a { color: var(--blue); }
    @media (max-width: 900px) {
      .top { align-items: flex-start; flex-direction: column; }
      .meta { text-align: left; }
      .summary-grid, .metric-grid { grid-template-columns: 1fr; }
      .bar-row { grid-template-columns: 1fr; gap: 4px; }
    }
  </style>
</head>
<body>
  <header>
    <div class="wrap">
      <div class="top">
        <h1>Anneal CI Metrics</h1>
        <div class="meta" id="meta"></div>
      </div>
      <nav id="tabs"></nav>
    </div>
  </header>
  <main class="wrap">
    <section id="overview"></section>
    <section id="timeline" hidden></section>
    <section id="archive" hidden></section>
    <section id="examples" hidden></section>
    <section id="legacy" hidden></section>
  </main>
  <script src="data.js"></script>
  <script>
    const data = window.ANNEAL_CI_METRICS || {};
    const registry = (data.registry && data.registry.metrics) || {};
    const series = data.series || {};
    const latestRun = data.latest && data.latest.run;
    const tabs = [
      ["overview", "Overview"],
      ["timeline", "Timeline"],
      ["archive", "Archive"],
      ["examples", "Examples"],
      ["legacy", "Legacy"]
    ];

    function escapeHtml(value) {
      return String(value ?? "").replace(/[&<>"']/g, ch => ({
        "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;"
      })[ch]);
    }

    function points(metricId) {
      return ((series[metricId] || {}).points || []).filter(p => typeof p.value === "number");
    }

    function latest(metricId) {
      const ps = points(metricId);
      return ps.length ? ps[ps.length - 1] : null;
    }

    function median(values) {
      if (!values.length) return null;
      const sorted = [...values].sort((a, b) => a - b);
      const mid = Math.floor(sorted.length / 2);
      return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
    }

    function baseline(metricId) {
      const ps = points(metricId);
      if (ps.length < 2) return null;
      return median(ps.slice(Math.max(0, ps.length - 15), -1).map(p => p.value));
    }

    function formatValue(value, unit) {
      if (value === null || value === undefined) return "n/a";
      if (unit === "seconds") {
        if (value >= 90) return `${(value / 60).toFixed(1)} min`;
        return `${Math.round(value)} s`;
      }
      if (unit === "bytes") {
        const units = ["B", "KiB", "MiB", "GiB"];
        let v = value;
        let i = 0;
        while (v >= 1024 && i < units.length - 1) { v /= 1024; i += 1; }
        return `${v.toFixed(i ? 1 : 0)} ${units[i]}`;
      }
      if (unit === "boolean") return value ? "yes" : "no";
      return `${value} ${unit || ""}`.trim();
    }

    function metricName(metricId) {
      if (registry[metricId] && registry[metricId].name) return registry[metricId].name;
      let match = metricId.match(/^job\\.verify_examples\\.([a-z0-9_]+)\\.seconds$/);
      if (match) return `Verify example (${match[1]})`;
      match = metricId.match(/^step\\.verify_examples\\.([a-z0-9_]+)\\.([a-z0-9_]+)\\.seconds$/);
      if (match) return `Verify ${match[1]} / ${match[2].replaceAll("_", " ")}`;
      return metricId;
    }

    function metricUnit(metricId) {
      return (registry[metricId] && registry[metricId].unit) || (latest(metricId) || {}).unit;
    }

    function delta(metricId) {
      const p = latest(metricId);
      const b = baseline(metricId);
      if (!p || b === null || b === 0) return "";
      const pct = ((p.value - b) / b) * 100;
      const direction = (registry[metricId] && registry[metricId].direction) || "smaller";
      const bad = direction === "smaller" ? pct > 0 : pct < 0;
      const cls = Math.abs(pct) < 1 ? "" : bad ? " bad" : " good";
      return `<div class="delta${cls}">${pct >= 0 ? "+" : ""}${pct.toFixed(1)}% vs 14-run median</div>`;
    }

    function summaryCard(metricId) {
      const p = latest(metricId);
      const unit = metricUnit(metricId);
      return `<div class="panel">
        <div class="label">${escapeHtml(metricName(metricId))}</div>
        <div class="value">${escapeHtml(formatValue(p && p.value, unit))}</div>
        ${delta(metricId)}
      </div>`;
    }

    function chart(metricId) {
      const ps = points(metricId).slice(-80);
      if (!ps.length) return '<div class="empty">No data yet</div>';
      const w = 640, h = 180, pad = 24;
      const values = ps.map(p => p.value);
      const min = Math.min(...values);
      const max = Math.max(...values);
      const span = Math.max(1, max - min);
      const coords = ps.map((p, i) => {
        const x = pad + (i * (w - pad * 2)) / Math.max(1, ps.length - 1);
        const y = h - pad - ((p.value - min) * (h - pad * 2)) / span;
        return [x, y];
      });
      const path = coords.map(([x, y]) => `${x.toFixed(1)},${y.toFixed(1)}`).join(" ");
      return `<svg class="chart" viewBox="0 0 ${w} ${h}" role="img" aria-label="${escapeHtml(metricName(metricId))}">
        <line class="axis" x1="${pad}" y1="${h - pad}" x2="${w - pad}" y2="${h - pad}"></line>
        <line class="axis" x1="${pad}" y1="${pad}" x2="${pad}" y2="${h - pad}"></line>
        <text x="${pad}" y="${pad - 6}">${escapeHtml(formatValue(max, metricUnit(metricId)))}</text>
        <text x="${pad}" y="${h - 6}">${escapeHtml(formatValue(min, metricUnit(metricId)))}</text>
        <polyline class="line" points="${path}"></polyline>
        ${coords.map(([x, y]) => `<circle class="dot" cx="${x.toFixed(1)}" cy="${y.toFixed(1)}" r="2.5"></circle>`).join("")}
      </svg>`;
    }

    function metricPanel(metricId) {
      const p = latest(metricId);
      const run = p && p.run;
      const link = run && run.html_url ? `<a href="${escapeHtml(run.html_url)}">run ${escapeHtml(run.run_id)}</a>` : "";
      return `<div class="panel">
        <h2>${escapeHtml(metricName(metricId))}</h2>
        <div class="delta">${escapeHtml(formatValue(p && p.value, metricUnit(metricId)))} ${link}</div>
        ${chart(metricId)}
      </div>`;
    }

    function renderOverview() {
      const summary = [
        "workflow.wall_seconds",
        "workflow.critical_path_seconds",
        "workflow.runner_seconds",
        "step.anneal_tests.run_all_tests.seconds",
        "archive.compressed_bytes"
      ];
      const panels = [
        "workflow.wall_seconds",
        "workflow.queue_seconds",
        "workflow.critical_path_seconds",
        "workflow.runner_seconds",
        "job.anneal_tests.seconds",
        "step.anneal_tests.run_all_tests.seconds"
      ];
      document.getElementById("overview").innerHTML = `
        <div class="grid summary-grid">${summary.map(summaryCard).join("")}</div>
        <div class="grid metric-grid">${panels.map(metricPanel).join("")}</div>`;
    }

    function renderTimeline() {
      const latest = data.latest || {};
      const jobs = (latest.jobs || []).filter(j => j.started_at && j.completed_at);
      if (!jobs.length) {
        document.getElementById("timeline").innerHTML = '<div class="panel empty">No job timing data yet</div>';
        return;
      }
      const times = jobs.flatMap(j => [Date.parse(j.started_at), Date.parse(j.completed_at)]).filter(Number.isFinite);
      const min = Math.min(...times), max = Math.max(...times), span = Math.max(1, max - min);
      const rows = jobs.sort((a, b) => Date.parse(a.started_at) - Date.parse(b.started_at)).map(job => {
        const left = ((Date.parse(job.started_at) - min) / span) * 100;
        const width = Math.max(0.5, ((Date.parse(job.completed_at) - Date.parse(job.started_at)) / span) * 100);
        const name = job.name || "";
        const cls = name.includes("Verify example") ? "verify" : name.includes("Archive") ? "archive" : name.includes("Static") ? "static" : "";
        const seconds = Math.round((Date.parse(job.completed_at) - Date.parse(job.started_at)) / 1000);
        return `<div class="bar-row">
          <div class="bar-name" title="${escapeHtml(name)}">${escapeHtml(name)}</div>
          <div class="bar-track"><div class="bar ${cls}" style="left:${left.toFixed(2)}%;width:${width.toFixed(2)}%"></div></div>
          <div>${escapeHtml(formatValue(seconds, "seconds"))}</div>
        </div>`;
      }).join("");
      document.getElementById("timeline").innerHTML = `<div class="panel"><h2>Latest Run Timeline</h2><div class="timeline">${rows}</div></div>`;
    }

    function renderArchive() {
      const metrics = [
        "job.nix_archive.seconds",
        "step.nix_archive.build_anneal_toolchain_archive.seconds",
        "archive.compressed_bytes",
        "archive.release_limit_headroom_bytes",
        "step.anneal_tests.download_anneal_toolchain_archive.seconds",
        "step.anneal_tests.install_anneal_toolchain_archive.seconds",
        "nix.main_cache.hit",
        "nix.pr_cache.hit"
      ];
      document.getElementById("archive").innerHTML = `<div class="grid metric-grid">${metrics.map(metricPanel).join("")}</div>`;
    }

    function renderExamples() {
      const aggregate = [
        "job.verify_examples.max_seconds",
        "job.verify_examples.p90_seconds",
        "job.verify_examples.p50_seconds",
        "job.verify_examples.runner_seconds"
      ];
      const exampleRows = Object.keys(series)
        .filter(id => id.startsWith("job.verify_examples.") && id.endsWith(".seconds") && !aggregate.includes(id))
        .sort()
        .map(id => {
          const p = latest(id);
          return `<tr><td>${escapeHtml(metricName(id))}</td><td>${escapeHtml(formatValue(p && p.value, "seconds"))}</td><td>${escapeHtml((p && p.tags && p.tags.example) || "")}</td></tr>`;
        }).join("");
      document.getElementById("examples").innerHTML = `
        <div class="grid summary-grid">${aggregate.map(summaryCard).join("")}</div>
        <div class="panel"><h2>Per-example Jobs</h2><table><thead><tr><th>Metric</th><th>Latest</th><th>Example</th></tr></thead><tbody>${exampleRows}</tbody></table></div>`;
    }

    function renderLegacy() {
      const legacy = data.legacy || {};
      const counts = {};
      for (const p of legacy.points || []) counts[p.metric_id] = (counts[p.metric_id] || 0) + 1;
      const rows = Object.keys(counts).sort().map(id => {
        const def = registry[id] || {};
        return `<tr><td>${escapeHtml(def.name || id)}</td><td>${escapeHtml(counts[id])}</td><td>${escapeHtml(def.retired_reason || "")}</td></tr>`;
      }).join("");
      document.getElementById("legacy").innerHTML = `<div class="panel"><h2>Retired Benchmark-action Series</h2><table><thead><tr><th>Series</th><th>Points</th><th>Status</th></tr></thead><tbody>${rows}</tbody></table></div>`;
    }

    function switchTab(id) {
      for (const [tabId] of tabs) document.getElementById(tabId).hidden = tabId !== id;
      for (const button of document.querySelectorAll("nav button")) button.classList.toggle("active", button.dataset.tab === id);
    }

    function renderTabs() {
      document.getElementById("tabs").innerHTML = tabs.map(([id, label], i) =>
        `<button data-tab="${id}" class="${i === 0 ? "active" : ""}">${escapeHtml(label)}</button>`).join("");
      for (const button of document.querySelectorAll("nav button")) button.addEventListener("click", () => switchTab(button.dataset.tab));
    }

    function renderMeta() {
      if (!latestRun) {
        document.getElementById("meta").textContent = `Generated ${data.generated_at || ""}`;
        return;
      }
      const sha = latestRun.head_sha ? String(latestRun.head_sha).slice(0, 9) : "";
      document.getElementById("meta").innerHTML = `${escapeHtml(latestRun.repository || "")}<br>${escapeHtml(latestRun.head_branch || "")} ${escapeHtml(sha)} - generated ${escapeHtml(data.generated_at || "")}`;
    }

    renderMeta();
    renderTabs();
    renderOverview();
    renderTimeline();
    renderArchive();
    renderExamples();
    renderLegacy();
  </script>
</body>
</html>
"""


def generate_dashboard(data_dir: Path, out_dir: Path, registry_path: Path = DEFAULT_REGISTRY) -> dict[str, Any]:
    payload = dashboard_payload(data_dir, registry_path)
    out_dir.mkdir(parents=True, exist_ok=True)
    (out_dir / "index.html").write_text(INDEX_HTML)
    write_json_js(out_dir / "data.js", "ANNEAL_CI_METRICS", payload)
    return payload


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--data-dir", required=True, type=Path)
    parser.add_argument("--out-dir", required=True, type=Path)
    parser.add_argument("--registry", default=DEFAULT_REGISTRY, type=Path)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    payload = generate_dashboard(args.data_dir, args.out_dir, args.registry)
    metric_count = len(payload.get("series") or {})
    print(f"Wrote Anneal CI dashboard with {metric_count} metric series to {args.out_dir}")


if __name__ == "__main__":
    main()
