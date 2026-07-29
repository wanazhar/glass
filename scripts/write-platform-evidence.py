#!/usr/bin/env python3
"""Write one revision-bound real-browser platform evidence report."""

import json
import hashlib
import os
import pathlib
import platform
import subprocess
import sys


REQUIRED_TARGETS = {
    "x86_64-unknown-linux-gnu": ("linux", "x86_64"),
    "aarch64-unknown-linux-gnu": ("linux", "aarch64"),
    "x86_64-apple-darwin": ("macos", "x86_64"),
    "aarch64-apple-darwin": ("macos", "aarch64"),
}


def command(*args: str) -> str:
    return subprocess.check_output(args, text=True).strip()


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: write-platform-evidence.py OUTPUT.json")

    target = os.environ.get("GLASS_PLATFORM_TARGET", "")
    if target not in REQUIRED_TARGETS:
        raise SystemExit(f"unsupported or missing GLASS_PLATFORM_TARGET: {target!r}")
    artifact = pathlib.Path(os.environ.get("GLASS_PLATFORM_ARTIFACT", ""))
    if not artifact.is_file():
        raise SystemExit(f"missing packaged platform artifact: {artifact}")

    version = next(
        package["version"]
        for package in json.loads(
            command("cargo", "metadata", "--no-deps", "--format-version", "1")
        )["packages"]
        if package["name"] == "glass-browser"
    )
    chrome = os.environ.get("GLASS_PLATFORM_CHROME", "managed-chromium")
    browser_version = os.environ.get("GLASS_PLATFORM_BROWSER_VERSION", "")
    if not browser_version:
        chrome_path = os.environ.get("CHROME_PATH", "")
        if chrome_path:
            browser_version = command(chrome_path, "--version")
    if not browser_version:
        raise SystemExit("missing browser version for platform evidence")
    raw_report = os.environ.get("GLASS_PLATFORM_RAW_REPORT", "browser-smoke.log")
    run_url = os.environ.get("GITHUB_RUN_URL", "local://glass-validation")
    os_name, architecture = REQUIRED_TARGETS[target]
    artifact_bytes = artifact.read_bytes()
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
        "platforms": [{
            "target": target,
            "os": os_name,
            "architecture": architecture,
            "chrome": chrome,
            "browser_version": browser_version,
            "runner": {
                "os": os.environ.get("RUNNER_OS", platform.system()),
                "architecture": os.environ.get("RUNNER_ARCH", platform.machine()),
                "image": os.environ.get("ImageOS", "not-recorded"),
                "image_version": os.environ.get("ImageVersion", "not-recorded"),
            },
            "artifact": {
                "name": os.environ.get("GLASS_ARTIFACT_NAME", artifact.name),
                "sha256": hashlib.sha256(artifact_bytes).hexdigest(),
                "size_bytes": len(artifact_bytes),
            },
            "status": "passed",
            "smoke_command": os.environ.get(
                "GLASS_PLATFORM_SMOKE_COMMAND",
                "cargo test --test browser_smoke --locked -- --nocapture --test-threads=1",
            ),
            "raw_report": raw_report,
        }],
    }
    output = pathlib.Path(sys.argv[1])
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
