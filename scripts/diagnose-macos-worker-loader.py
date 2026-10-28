#!/usr/bin/env python3
"""Capture a bounded, path-redacted macOS sandboxed worker loader probe."""

from __future__ import annotations

import argparse
import json
import os
import platform
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

MAX_CAPTURE_BYTES = 64 * 1024
MAX_IMAGES = 16
SAFE_TOKEN = re.compile(r"^[A-Za-z0-9_.$<>+-]{1,160}$")
IMAGE_PATH = re.compile(
    r"/(?:[^\s:'\"<>]+/)*[^\s:'\"<>]+(?:\.dylib|\.framework(?:/[^\s:'\"<>]+)?)"
)
ERROR_PATTERNS = (
    (re.compile(r"\b(?:wrong|incompatible) architecture\b", re.IGNORECASE), "Wrong architecture"),
    (re.compile(r"\bno suitable image found\b", re.IGNORECASE), "No suitable image found"),
    (re.compile(r"\b(?:library not loaded|library missing|dependent dylib).*\bnot found\b", re.IGNORECASE), "Library missing"),
    (re.compile(r"\b(?:symbol not found|symbol missing)\b", re.IGNORECASE), "Symbol missing"),
    (re.compile(r"\bwrong version\b", re.IGNORECASE), "Wrong version"),
    (re.compile(r"\bmalformed mach[- ]o\b", re.IGNORECASE), "Malformed Mach-O"),
    (re.compile(r"\bcode signature\b", re.IGNORECASE), "Code signature"),
    (re.compile(r"\b(?:operation not permitted|permission denied|sandbox deny)\b", re.IGNORECASE), "Access denied"),
)


def safe_token(value: str) -> str | None:
    return value if SAFE_TOKEN.fullmatch(value) else None


def architectures_for(path: Path) -> list[str]:
    try:
        result = subprocess.run(
            ["/usr/bin/lipo", "-archs", str(path)],
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=3,
            check=False,
            text=True,
        )
    except (OSError, subprocess.TimeoutExpired):
        return []
    if result.returncode != 0:
        return []
    return [token for token in result.stdout.split() if safe_token(token)]


def sandbox_profile(worker: Path) -> str:
    path = str(worker).replace("\\", "\\\\").replace('"', '\\"')
    return (
        '(version 1) (deny default) '
        f'(allow process-exec (literal "{path}")) '
        f'(allow file-read* (subpath "/System") (subpath "/usr") '
        f'(subpath "/Library") (literal "{path}")) '
        '(allow network-outbound) (allow sysctl-read) (allow mach-lookup)'
    )


def run_probe(worker: Path) -> dict[str, Any]:
    environment = os.environ.copy()
    environment["DYLD_PRINT_LIBRARIES"] = "1"
    environment["DYLD_PRINT_RPATHS"] = "1"
    host_architecture = safe_token(platform.machine())
    binary_architectures = architectures_for(worker)
    command = [
        "/usr/bin/sandbox-exec",
        "-p",
        sandbox_profile(worker),
        str(worker),
        "--native-content-worker",
    ]
    report: dict[str, Any] = {
        "probe": "macos-sandboxed-worker-loader",
        "binary": worker.name,
        "hostArchitecture": host_architecture,
        "binaryArchitectures": binary_architectures,
        "binaryArchitectureMatchesHost": (
            host_architecture in binary_architectures
            if host_architecture and binary_architectures
            else None
        ),
        "status": "unknown",
        "capturedBytes": 0,
        "captureTruncated": False,
        "loaderLineCount": 0,
        "images": [],
        "errorCategories": [],
    }
    try:
        result = subprocess.run(
            command,
            input=b"",
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=environment,
            timeout=10,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        stderr = error.stderr or b""
        report["status"] = "timeout"
    except OSError:
        stderr = b""
        report["status"] = "launch-error"
    else:
        stderr = result.stderr
        report["status"] = (
            f"signal-{-result.returncode}"
            if result.returncode < 0
            else f"exit-{result.returncode}"
        )

    report["capturedBytes"] = min(len(stderr), MAX_CAPTURE_BYTES)
    report["captureTruncated"] = len(stderr) > MAX_CAPTURE_BYTES
    full_text = stderr.decode("utf-8", errors="replace")
    report["loaderLineCount"] = len(full_text.splitlines())
    stderr = stderr[-MAX_CAPTURE_BYTES:]
    lines = stderr.decode("utf-8", errors="replace").splitlines()
    report["errorCategories"] = list(
        dict.fromkeys(
            label for pattern, label in ERROR_PATTERNS if pattern.search(full_text)
        )
    )

    images: list[dict[str, Any]] = []
    for line in lines:
        # dyld print output contains full image paths; retain only safe basenames.
        for match in IMAGE_PATH.finditer(line):
            image_path = Path(match.group(0).rstrip(",.;:)"))
            name = safe_token(image_path.name)
            if not name or any(image["name"] == name for image in images):
                continue
            architecture_target = image_path
            if image_path.suffix == ".framework" and image_path.is_dir():
                architecture_target = image_path / image_path.stem
            architectures = architectures_for(architecture_target)
            images.append(
                {
                    "name": name,
                    "architectures": architectures,
                    "architectureMatchesHost": (
                        platform.machine() in architectures if architectures else None
                    ),
                }
            )
            if len(images) >= MAX_IMAGES:
                break
        if len(images) >= MAX_IMAGES:
            break
    report["images"] = images
    return report


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    arguments = parser.parse_args()
    report = run_probe(arguments.binary.resolve())
    arguments.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(f"sanitized macOS loader probe: {report['status']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
