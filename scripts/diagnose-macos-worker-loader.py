#!/usr/bin/env python3
"""Capture a bounded, path-redacted macOS sandboxed worker loader probe."""

from __future__ import annotations

import argparse
import json
import os
import platform
import re
import subprocess
from pathlib import Path
from typing import Any

MAX_CAPTURE_BYTES = 32 * 1024
MAX_IMAGES = 12
SAFE_TOKEN = re.compile(r"^[A-Za-z0-9_.$<>+-]{1,160}$")
DENIAL_EVENT = re.compile(
    r"Sandbox:\s*[A-Za-z0-9_.-]+\((\d+)\)\s+deny\([^)]*\)\s+([A-Za-z][A-Za-z0-9*-]*)(?:\s+(.*))?"
)
DENIED_PATH = re.compile(r"(?<![A-Za-z0-9])(/[^\x00\s\"'<>),;]*)")
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
PATH_CLASS_ROOTS = {
    "System": ("/System",),
    "usr": ("/usr",),
    "Library": ("/Library",),
    "privatevar": ("/private/var", "/var"),
    "dev": ("/dev",),
    "etc": ("/etc",),
}


def denied_paths(raw_path: str | None) -> list[str]:
    if not raw_path:
        return []
    return [
        candidate.rstrip(".,;:)")
        for candidate in DENIED_PATH.findall(raw_path)
        if len(candidate) <= 1024
    ]


def denied_path_class(raw_path: str | None) -> str:
    paths = denied_paths(raw_path)
    for path_class, roots in PATH_CLASS_ROOTS.items():
        if any(
            path == root or path.startswith(root + "/")
            for path in paths
            for root in roots
        ):
            return path_class
    return "other"


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


def sandbox_profile(
    binary: Path,
    *,
    addition: str | None = None,
    addition_path: str | None = None,
) -> str:
    path = str(binary).replace("\\", "\\\\").replace('"', '\\"')
    profile = (
        '(version 1) (deny default) '
        f'(allow process-exec (literal "{path}")) '
        f'(allow file-read* (subpath "/System") (subpath "/usr") '
        f'(subpath "/Library") (literal "{path}")) '
        '(allow network-outbound) (allow sysctl-read) (allow mach-lookup)'
    )
    file_read_data_roots = {
        f"file-read-data-{path_class.lower()}": roots
        for path_class, roots in PATH_CLASS_ROOTS.items()
    }
    if addition == "file-map-system":
        profile += (
            ' (allow file-map-executable (subpath "/System") '
            '(subpath "/usr") (subpath "/Library") '
            f'(literal "{path}"))'
        )
    elif addition == "file-read-metadata-system":
        profile += (
            ' (allow file-read-metadata (subpath "/System") '
            '(subpath "/usr") (subpath "/Library") '
            f'(literal "{path}"))'
        )
    elif addition in file_read_data_roots:
        paths = " ".join(
            f'(subpath "{root}")' for root in file_read_data_roots[addition]
        )
        profile += f" (allow file-read-data {paths})"
    elif addition == "file-read-data-exact" and addition_path:
        exact_path = addition_path.replace("\\", "\\\\").replace('"', '\\"')
        profile += f' (allow file-read-data (literal "{exact_path}"))'
    elif addition is not None:
        raise ValueError("unsupported diagnostic sandbox rule")
    return profile


def clean_environment(*, dyld_print: bool) -> dict[str, str]:
    environment = {"PATH": "/usr/bin:/bin:/usr/sbin:/sbin"}
    if dyld_print:
        environment["DYLD_PRINT_LIBRARIES"] = "1"
        environment["DYLD_PRINT_RPATHS"] = "1"
    return environment


def run_launch(
    binary: Path,
    *,
    sandboxed: bool,
    dyld_print: bool,
    worker: bool,
    profile: str = "production",
    addition_path: str | None = None,
) -> dict[str, Any]:
    command = [str(binary)]
    if worker:
        command.append("--native-content-worker")
    if sandboxed:
        profile_additions = {
            "production-plus-file-map": "file-map-system",
            "production-plus-file-read-metadata": "file-read-metadata-system",
        }
        profile_additions.update(
            {
                f"production-plus-file-read-data-{path_class.lower()}": (
                    f"file-read-data-{path_class.lower()}"
                )
                for path_class in PATH_CLASS_ROOTS
            }
        )
        if profile == "permissive-control":
            profile_text = "(version 1) (allow default)"
        elif profile.startswith("production-plus-file-read-data-exact-"):
            profile_text = sandbox_profile(
                binary,
                addition="file-read-data-exact",
                addition_path=addition_path,
            )
        else:
            profile_text = sandbox_profile(
                binary, addition=profile_additions.get(profile)
            )
        command = [
            "/usr/bin/sandbox-exec",
            "-p",
            profile_text,
            *command,
        ]
    process_id: int | None = None
    stderr = b""
    try:
        process = subprocess.Popen(
            command,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=clean_environment(dyld_print=dyld_print),
        )
        process_id = process.pid
        try:
            _, stderr = process.communicate(input=b"", timeout=10)
        except subprocess.TimeoutExpired:
            process.kill()
            _, stderr = process.communicate()
            status = "timeout"
        else:
            return_code = process.returncode
            status = (
                f"signal-{-return_code}"
                if return_code < 0
                else f"exit-{return_code}"
            )
    except OSError:
        status = "launch-error"

    report: dict[str, Any] = {
        "binary": binary.name,
        "sandboxed": sandboxed,
        "sandboxProfile": profile if sandboxed else "none",
        "dyldPrint": dyld_print,
        "status": status,
        "capturedBytes": min(len(stderr), MAX_CAPTURE_BYTES),
        "captureTruncated": len(stderr) > MAX_CAPTURE_BYTES,
        "loaderLineCount": 0,
        "images": [],
        "errorCategories": [],
    }
    report["_processId"] = process_id
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


def sandbox_denials(
    process_cases: dict[int, str],
) -> tuple[dict[str, Any], dict[str, list[str]]]:
    if not process_cases:
        return {"status": "unavailable", "cases": {}}, {}
    pid_predicate = " OR ".join(
        f'(processID == {process_id} OR eventMessage CONTAINS "({process_id})")'
        for process_id in sorted(process_cases)
    )
    predicate = (
        'subsystem == "com.apple.sandbox.reporting" '
        'AND category == "violation" '
        f"AND ({pid_predicate})"
    )
    try:
        result = subprocess.run(
            [
                "/usr/bin/log",
                "show",
                "--style",
                "json",
                "--last",
                "2m",
                "--predicate",
                predicate,
            ],
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            timeout=12,
            check=False,
            text=True,
        )
    except (OSError, subprocess.TimeoutExpired):
        return {"status": "unavailable", "cases": {}}, {}
    if result.returncode != 0:
        return {"status": "unavailable", "cases": {}}, {}
    try:
        events = json.loads(result.stdout)
    except json.JSONDecodeError:
        return {"status": "unavailable", "cases": {}}, {}

    operation_allowlist = {
        "file-read-data",
        "file-read*",
        "file-read-metadata",
        "file-map-executable",
        "file-test-existence",
        "file-ioctl",
        "process-exec",
        "process-exec*",
        "process-fork",
        "process-info",
        "process-info*",
        "mach-lookup",
        "mach-register",
        "mach-task-name",
        "network-outbound",
        "sysctl-read",
        "sysctl-write",
        "system-socket",
        "ipc-posix-sem",
        "ipc-posix-shm-read-data",
        "ipc-posix-shm-write-create",
        "signal",
        "iokit-open",
    }
    counts: dict[str, dict[str, int]] = {name: {} for name in process_cases.values()}
    path_classes = (*PATH_CLASS_ROOTS, "other")
    path_counts = {
        name: {path_class: 0 for path_class in path_classes}
        for name in process_cases.values()
    }
    file_read_paths: dict[str, list[str]] = {
        name: [] for name in process_cases.values()
    }
    matched = 0
    if isinstance(events, dict):
        events = events.get("events", [events])
    for event in events if isinstance(events, list) else []:
        if not isinstance(event, dict):
            continue
        message = event.get("eventMessage")
        if not isinstance(message, str):
            continue
        match = DENIAL_EVENT.search(message)
        if not match:
            continue
        case_name = process_cases.get(int(match.group(1)))
        if case_name is None:
            continue
        operation = match.group(2)
        event_paths = denied_paths(match.group(3))
        if operation == "file-read-data":
            for path in event_paths:
                if path not in file_read_paths[case_name] and len(
                    file_read_paths[case_name]
                ) < 3:
                    file_read_paths[case_name].append(path)
        path_counts[case_name][denied_path_class(match.group(3))] += 1
        if operation not in operation_allowlist:
            operation = "other"
        counts[case_name][operation] = counts[case_name].get(operation, 0) + 1
        matched += 1
    return (
        {
            "status": "available" if matched else "no-matching-events",
            "cases": counts,
            "pathClasses": path_counts,
        },
        file_read_paths,
    )


def signing_and_quarantine(worker: Path) -> dict[str, str]:
    try:
        signature = subprocess.run(
            ["/usr/bin/codesign", "--verify", "--strict", str(worker)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=5,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        signature_status = "unavailable"
    else:
        signature_status = "valid" if signature.returncode == 0 else "invalid"

    try:
        quarantine = subprocess.run(
            ["/usr/bin/xattr", "-p", "com.apple.quarantine", str(worker)],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=5,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired):
        quarantine_status = "unavailable"
    else:
        quarantine_status = "present" if quarantine.returncode == 0 else "absent"
    return {"signature": signature_status, "quarantine": quarantine_status}


def run_probe(worker: Path) -> dict[str, Any]:
    host_architecture = safe_token(platform.machine())
    binary_architectures = architectures_for(worker)
    cases = (
        ("worker-unsandboxed-clean", worker, False, False, True),
        ("worker-sandboxed-clean", worker, True, False, True),
        ("worker-unsandboxed-dyld-print", worker, False, True, True),
        ("worker-sandboxed-dyld-print", worker, True, True, True),
        ("known-good-unsandboxed-clean", Path("/usr/bin/true"), False, False, False),
        ("known-good-sandboxed-clean", Path("/usr/bin/true"), True, False, False),
        (
            "known-good-sandboxed-permissive-control",
            Path("/usr/bin/true"),
            True,
            False,
            False,
            "permissive-control",
        ),
        (
            "known-good-sandboxed-production-plus-file-map",
            Path("/usr/bin/true"),
            True,
            False,
            False,
            "production-plus-file-map",
        ),
        (
            "worker-sandboxed-production-plus-file-map",
            worker,
            True,
            False,
            True,
            "production-plus-file-map",
        ),
        (
            "known-good-sandboxed-production-plus-file-read-metadata",
            Path("/usr/bin/true"),
            True,
            False,
            False,
            "production-plus-file-read-metadata",
        ),
    )
    cases = list(cases)
    for path_class in PATH_CLASS_ROOTS:
        profile_name = f"production-plus-file-read-data-{path_class.lower()}"
        cases.extend(
            (
                (
                    f"known-good-sandboxed-{profile_name}",
                    Path("/usr/bin/true"),
                    True,
                    False,
                    False,
                    profile_name,
                ),
                (
                    f"worker-sandboxed-{profile_name}",
                    worker,
                    True,
                    False,
                    True,
                    profile_name,
                ),
            )
        )
    case_reports: list[dict[str, Any]] = []
    process_cases: dict[int, str] = {}
    for case in cases:
        launch_report = run_launch(
            case[1],
            sandboxed=case[2],
            dyld_print=case[3],
            worker=case[4],
            profile=case[5] if len(case) > 5 else "production",
            addition_path=case[6] if len(case) > 6 else None,
        )
        process_id = launch_report.pop("_processId", None)
        if isinstance(process_id, int):
            process_cases[process_id] = case[0]
        case_reports.append({"name": case[0], **launch_report})

    _, observed_file_read_paths = sandbox_denials(process_cases)
    exact_path_candidates: list[str] = []
    for source_case in (
        "known-good-sandboxed-clean",
        "worker-sandboxed-clean",
    ):
        for path in observed_file_read_paths.get(source_case, []):
            if path not in exact_path_candidates and len(exact_path_candidates) < 3:
                exact_path_candidates.append(path)

    exact_path_status = "available" if exact_path_candidates else "no-denied-path"
    for index, exact_path in enumerate(exact_path_candidates, start=1):
        profile_name = f"production-plus-file-read-data-exact-{index}"
        for case_name, binary, worker_case in (
            (f"known-good-sandboxed-{profile_name}", Path("/usr/bin/true"), False),
            (f"worker-sandboxed-{profile_name}", worker, True),
        ):
            launch_report = run_launch(
                binary,
                sandboxed=True,
                dyld_print=False,
                worker=worker_case,
                profile=profile_name,
                addition_path=exact_path,
            )
            process_id = launch_report.pop("_processId", None)
            if isinstance(process_id, int):
                process_cases[process_id] = case_name
            case_reports.append({"name": case_name, **launch_report})

    exact_path_candidates.clear()
    observed_file_read_paths.clear()
    denial_summary, _ = sandbox_denials(process_cases)
    for case_report in case_reports:
        case_report["sandboxDeniedOperations"] = denial_summary["cases"].get(
            case_report["name"], {}
        )
        case_report["sandboxDeniedPathClasses"] = denial_summary[
            "pathClasses"
        ].get(case_report["name"], {})
    return {
        "probe": "macos-native-worker-launch-matrix",
        "hostArchitecture": host_architecture,
        "workerArchitectures": binary_architectures,
        "workerArchitectureMatchesHost": (
            host_architecture in binary_architectures
            if host_architecture and binary_architectures
            else None
        ),
        **signing_and_quarantine(worker),
        "sandboxDenialLogStatus": denial_summary["status"],
        "exactPathDiagnosticStatus": exact_path_status,
        "cases": case_reports,
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    arguments = parser.parse_args()
    report = run_probe(arguments.binary.resolve())
    arguments.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    clean_sandboxed = report["cases"][1]["status"]
    print(f"sanitized macOS launch matrix: {clean_sandboxed}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
