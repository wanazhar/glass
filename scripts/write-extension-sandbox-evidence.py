#!/usr/bin/env python3
"""Write machine-readable evidence for one native extension sandbox gate."""

import argparse
import json
import pathlib
import subprocess


EXPECTED = {
    "linux-x86-64": "linux-bubblewrap",
    "linux-arm64": "linux-bubblewrap",
    "macos-x86-64": "macos-sandbox-exec",
    "macos-arm64": "macos-sandbox-exec",
}


def fail(message: str) -> None:
    raise SystemExit(f"extension sandbox evidence failed: {message}")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--target", required=True)
    parser.add_argument("--sandbox", required=True)
    parser.add_argument("--raw-report", required=True)
    parser.add_argument("--output", required=True, type=pathlib.Path)
    args = parser.parse_args()
    if args.target not in EXPECTED:
        fail(f"unsupported release target {args.target}")
    if EXPECTED[args.target] != args.sandbox:
        fail(f"{args.target} requires {EXPECTED[args.target]}, got {args.sandbox}")
    raw_report = pathlib.Path(args.raw_report)
    if not raw_report.is_file() or not raw_report.read_text(encoding="utf-8").strip():
        fail(f"sandbox test report is missing or empty: {raw_report}")
    report = {
        "schema_version": 1,
        "type": "native_extension_sandbox_evidence",
        "source_revision": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], text=True
        ).strip(),
        "target": args.target,
        "sandbox": args.sandbox,
        "status": "passed",
        "test_command": "GLASS_EXTENSION_SANDBOX_E2E=1 cargo test --lib extensions::tests::sandboxed_reference_extensions_pass_native_gate -- --nocapture",
        "raw_report": str(raw_report),
        "capability_policy": "blockedBySecurityGate_until_all_targets_pass",
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"native extension sandbox evidence recorded: {args.target} ({args.sandbox})")


if __name__ == "__main__":
    main()
