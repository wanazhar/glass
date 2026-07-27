#!/usr/bin/env python3
"""Derive ratified gate metrics from controlled benchmark reports."""

from __future__ import annotations

import argparse
import json
import math
from pathlib import Path


LIMITS = {
    "representative_task_success_rate": 0.95,
    "fresh_compact_observe_p95_ms": 5.0,
    "cached_compact_observe_p95_ms": 0.1,
    "fast_action_client_overhead_p95_ms": 5.0,
    "idle_glass_rss_bytes": 8 * 1024 * 1024,
    "mcp_malformed_input_survival_rate": 1.0,
}


def positive_number(value: object, label: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"{label} must be numeric")
    number = float(value)
    if not math.isfinite(number) or number < 0:
        raise ValueError(f"{label} must be finite and non-negative")
    return number


def result_by_name(report: dict, name: str) -> dict:
    for result in report.get("results", []):
        if result.get("operation") == name:
            return result
    raise ValueError(f"benchmark report is missing {name}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("benchmark", type=Path)
    parser.add_argument("scorecard", type=Path)
    parser.add_argument("--mcp-survival-rate", type=float, required=True)
    parser.add_argument("--mcp-raw-report", required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()

    benchmark = json.loads(args.benchmark.read_text(encoding="utf-8"))
    scorecard = json.loads(args.scorecard.read_text(encoding="utf-8"))
    summary = scorecard.get("summary", {})
    memory = benchmark.get("glass_process_memory", {})
    overhead = result_by_name(benchmark, "client_overhead")
    metrics = {
        "representative_task_success_rate": positive_number(
            summary.get("task_success_rate"), "task_success_rate"
        ),
        "fresh_compact_observe_p95_ms": positive_number(
            result_by_name(benchmark, "observe_compact_fresh").get("p95_ms"),
            "fresh_compact_observe_p95_ms",
        ),
        "cached_compact_observe_p95_ms": positive_number(
            result_by_name(benchmark, "observe_compact_cached").get("p95_ms"),
            "cached_compact_observe_p95_ms",
        ),
        "fast_action_client_overhead_p95_ms": positive_number(
            overhead.get("glass_overhead", {}).get("p95_ms"),
            "fast_action_client_overhead_p95_ms",
        ),
        "idle_glass_rss_bytes": positive_number(
            memory.get("rss_bytes_after_fast_start"), "idle_glass_rss_bytes"
        ),
        "mcp_malformed_input_survival_rate": positive_number(
            args.mcp_survival_rate, "mcp_malformed_input_survival_rate"
        ),
    }
    raw_reports = {
        "representative_task_success_rate": str(args.scorecard),
        "fresh_compact_observe_p95_ms": str(args.benchmark),
        "cached_compact_observe_p95_ms": str(args.benchmark),
        "fast_action_client_overhead_p95_ms": str(args.benchmark),
        "idle_glass_rss_bytes": str(args.benchmark),
        "mcp_malformed_input_survival_rate": args.mcp_raw_report,
    }
    passed = (
        summary.get("hard_gate_passed") is True
        and summary.get("wrong_actions") == 0
        and all(
            value >= LIMITS[name] if name in {
                "representative_task_success_rate",
                "mcp_malformed_input_survival_rate",
            } else value <= LIMITS[name]
            for name, value in metrics.items()
        )
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(
        json.dumps({"passed": passed, "metrics": metrics, "raw_reports": raw_reports}, indent=2)
        + "\n",
        encoding="utf-8",
    )
    print(json.dumps({"passed": passed, "metrics": metrics}, indent=2))
    return 0 if passed else 1


if __name__ == "__main__":
    raise SystemExit(main())
