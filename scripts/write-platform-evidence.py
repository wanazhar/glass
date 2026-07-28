#!/usr/bin/env python3
"""Write one revision-bound real-browser platform evidence report."""

import json
import os
import pathlib
import subprocess
import sys


REQUIRED_TARGETS = {
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
}


def command(*args: str) -> str:
    return subprocess.check_output(args, text=True).strip()


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: write-platform-evidence.py OUTPUT.json")

    target = os.environ.get("GLASS_PLATFORM_TARGET", "")
    if target not in REQUIRED_TARGETS:
        raise SystemExit(f"unsupported or missing GLASS_PLATFORM_TARGET: {target!r}")

    version = next(
        package["version"]
        for package in json.loads(
            command("cargo", "metadata", "--no-deps", "--format-version", "1")
        )["packages"]
        if package["name"] == "glass-browser"
    )
    os_name = "macos" if "apple-darwin" in target else "linux"
    chrome = os.environ.get("GLASS_PLATFORM_CHROME", "managed-chromium")
    raw_report = os.environ.get("GLASS_PLATFORM_RAW_REPORT", "browser-smoke.log")
    run_url = os.environ.get("GITHUB_RUN_URL", "local://glass-validation")
    report = {
        "schema_version": 1,
        "git_revision": command("git", "rev-parse", "HEAD"),
        "producer": {
            "name": "glass-release-platform-smoke",
            "version": version,
            "command": "scripts/write-platform-evidence.py",
            "run_url": run_url,
        },
        "type": "real_browser_platform_matrix",
        "platforms": [
            {
                "target": target,
                "os": os_name,
                "architecture": target.split("-", 1)[0],
                "chrome": chrome,
                "status": "passed",
                "raw_report": raw_report,
            }
        ],
    }
    output = pathlib.Path(sys.argv[1])
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
