#!/usr/bin/env python3
"""Write a bounded, privacy-safe summary of a macOS native-worker crash."""

from __future__ import annotations

import argparse
import json
import re
import stat
import sys
import tempfile
import threading
import time
from pathlib import Path
from typing import Any

MAX_REPORT_BYTES = 8 * 1024 * 1024
MAX_CANDIDATES = 24
MAX_FRAMES = 32
MAX_OUTPUT_BYTES = 16 * 1024
WORKER_PROCESS_NAME = "glass-native-content-worker"
WORKER_PROCESS_PREFIX = "glass-native-con"
SAFE_TOKEN = re.compile(r"^[A-Za-z0-9_.$<>:+-]{1,160}$")
OLD_FRAME = re.compile(
    r"^\s*(\d+)\s+([^\s]+)\s+0x[0-9a-fA-F]+\s+(.+?)(?:\s+\+\s+\d+)?\s*$"
)


def safe_token(value: Any, *, limit: int = 160) -> str | None:
    if not isinstance(value, str):
        return None
    value = value.strip()
    if len(value) > limit or not SAFE_TOKEN.fullmatch(value):
        return None
    return value


def image_name(value: Any) -> str | None:
    if not isinstance(value, str):
        return None
    name = value.replace("\\", "/").rsplit("/", 1)[-1]
    return safe_token(name, limit=100)


def report_process_name(payload: dict[str, Any]) -> str | None:
    for key in ("procName", "procPath", "app_name"):
        name = image_name(payload.get(key))
        if name:
            return name
    return None


def is_glass_related_process(payload: dict[str, Any]) -> bool:
    name = report_process_name(payload)
    return bool(name and ("glass" in name.casefold() or "native" in name.casefold()))


def is_worker_process(value: Any) -> bool:
    if not isinstance(value, str):
        return False
    value = value.replace("\\", "/").rsplit("/", 1)[-1].replace("_", "-")
    if value == WORKER_PROCESS_NAME:
        return True
    # macOS may expose a 16-character procName even though the executable's
    # full basename is longer. Accept only this fixed, distinctive prefix.
    if (
        len(WORKER_PROCESS_PREFIX) <= len(value) <= 20
        and WORKER_PROCESS_NAME.startswith(value)
    ):
        return True
    # Some launch paths use a more specific native-worker basename. Match
    # only Glass-native executables explicitly ending in `worker`.
    return (
        value.startswith("glass-native-")
        and value.endswith("-worker")
        and len(value) <= 64
    )


def ips_payloads(text: str) -> list[dict[str, Any]]:
    candidates: list[dict[str, Any]] = []
    lines = text.splitlines()
    for line in lines[:64]:
        try:
            value = json.loads(line)
        except (json.JSONDecodeError, TypeError):
            continue
        if isinstance(value, dict):
            candidates.append(value)

    # Newer macOS `.ips` files can contain a one-line metadata object followed
    # by a pretty-printed report object. Parse the report tail as a whole too;
    # otherwise the valid metadata line prevents the multiline crash payload
    # from ever being considered.
    for start in range(min(len(lines), 2)):
        try:
            value = json.loads("\n".join(lines[start:]))
        except (json.JSONDecodeError, TypeError):
            continue
        if isinstance(value, dict):
            if value not in candidates:
                candidates.append(value)
            break
    return candidates


def ips_payload(text: str) -> dict[str, Any] | None:
    candidates = ips_payloads(text)
    worker_name = next(
        (
            name
            for value in reversed(candidates)
            for name in (value.get("procName"), value.get("procPath"), value.get("app_name"))
            if is_worker_process(name)
        ),
        None,
    )
    if worker_name:
        # Some `.ips` files put the process name in a small first-line
        # metadata object and the exception/thread data in the report body.
        # Merge both bounded objects before selecting the crash summary.
        merged: dict[str, Any] = {}
        for value in candidates:
            merged.update(value)
        if not any(
            is_worker_process(merged.get(key))
            for key in ("procName", "procPath", "app_name")
        ):
            merged["procName"] = worker_name
        return merged
    # Preserve a bounded, allowlisted summary for a renamed Glass process so
    # the CI artifact can identify a missed executable alias without retaining
    # report text, paths, arguments, or arbitrary exception messages.
    return next(
        (value for value in reversed(candidates) if is_glass_related_process(value)),
        None,
    )


def ips_process_reports(text: str) -> list[dict[str, str]]:
    reports: list[dict[str, str]] = []
    for payload in reversed(ips_payloads(text)):
        process = report_process_name(payload)
        if not process:
            continue
        report = {"process": process}
        exception = payload.get("exception")
        if isinstance(exception, dict):
            for source, destination in (("type", "exception"), ("signal", "signal")):
                token = safe_token(exception.get(source), limit=64)
                if token:
                    report[destination] = token
        if report not in reports:
            reports.append(report)
        if len(reports) >= 8:
            break
    return reports


def ips_process_reports_at(path: Path, *, since: float) -> list[dict[str, str]]:
    try:
        metadata = path.lstat()
        if (
            stat.S_ISLNK(metadata.st_mode)
            or not stat.S_ISREG(metadata.st_mode)
            or metadata.st_mtime < since
            or metadata.st_size <= 0
            or metadata.st_size > MAX_REPORT_BYTES
        ):
            return []
        with path.open("rb") as report:
            raw = report.read(MAX_REPORT_BYTES + 1)
    except OSError:
        return []
    if len(raw) > MAX_REPORT_BYTES:
        return []
    return ips_process_reports(raw.decode("utf-8", errors="replace"))


def ips_summary(payload: dict[str, Any]) -> dict[str, Any]:
    worker_process = any(
        is_worker_process(payload.get(key))
        for key in ("procName", "procPath", "app_name")
    )
    summary: dict[str, Any] = {
        "capture": "matched" if worker_process else "related-process-report",
        "process": WORKER_PROCESS_NAME if worker_process else report_process_name(payload),
        "frames": [],
    }
    exception = payload.get("exception")
    if isinstance(exception, dict):
        for key in ("type", "signal"):
            token = safe_token(exception.get(key), limit=64)
            if token:
                summary["exception" if key == "type" else "signal"] = token

    termination = payload.get("termination")
    if isinstance(termination, dict):
        for key in ("namespace", "code"):
            token = safe_token(termination.get(key), limit=64)
            if token:
                summary[f"termination{key.title()}"] = token

    crashed_thread = payload.get("faultingThread")
    if isinstance(crashed_thread, int) and 0 <= crashed_thread < 10_000:
        summary["crashedThread"] = crashed_thread

    images = payload.get("usedImages")
    images = images if isinstance(images, list) else []
    threads = payload.get("threads")
    # Keep the report schema observable without retaining its arbitrary values.
    # A matched `.ips` may contain only the process metadata header; these
    # bounded booleans and counts distinguish that from an empty/unrecognized
    # thread stack and guide the next hosted capture change.
    summary["reportShape"] = {
        "exceptionPresent": isinstance(exception, dict),
        "terminationPresent": isinstance(termination, dict),
        "faultingThreadPresent": isinstance(crashed_thread, int),
        "threadsPresent": isinstance(threads, list),
        "threadCount": min(len(threads), 10_000) if isinstance(threads, list) else 0,
        "usedImagesPresent": isinstance(payload.get("usedImages"), list),
        "usedImageCount": min(len(images), 10_000),
        "crashDataPresent": (
            isinstance(exception, dict)
            or isinstance(termination, dict)
            or isinstance(crashed_thread, int)
            or isinstance(threads, list)
        ),
    }
    if not isinstance(threads, list):
        return summary

    thread: dict[str, Any] | None = None
    if isinstance(crashed_thread, int) and 0 <= crashed_thread < len(threads):
        candidate = threads[crashed_thread]
        if isinstance(candidate, dict):
            thread = candidate
    if thread is None:
        thread = next(
            (
                candidate
                for candidate in threads
                if isinstance(candidate, dict) and candidate.get("triggered") is True
            ),
            None,
        )
    if thread is None:
        return summary

    frames = thread.get("frames")
    if not isinstance(frames, list):
        return summary
    safe_frames: list[dict[str, Any]] = []
    for index, frame in enumerate(frames[:MAX_FRAMES]):
        if not isinstance(frame, dict):
            continue
        image_index = frame.get("imageIndex")
        image = None
        if isinstance(image_index, int) and 0 <= image_index < len(images):
            image_record = images[image_index]
            if isinstance(image_record, dict):
                image = image_name(image_record.get("name"))
        symbol = safe_token(frame.get("symbol"))
        symbol_location = frame.get("symbolLocation")
        item: dict[str, Any] = {"frame": index}
        if image:
            item["image"] = image
        if symbol:
            item["symbol"] = symbol
        if isinstance(symbol_location, int) and 0 <= symbol_location < 2**32:
            item["symbolOffset"] = symbol_location
        if len(item) > 1:
            safe_frames.append(item)
    summary["frames"] = safe_frames
    return summary


def old_crash_summary(text: str) -> dict[str, Any] | None:
    process: str | None = None
    exception: str | None = None
    crashed = False
    frames: list[dict[str, Any]] = []
    for line in text.splitlines()[:100_000]:
        if line.startswith("Process:"):
            match = re.match(r"Process:\s+([^\s]+)", line)
            if match:
                process = image_name(match.group(1))
        elif line.startswith("Exception Type:"):
            match = re.match(r"Exception Type:\s+([^\s(]+)", line)
            if match:
                exception = safe_token(match.group(1), limit=64)
        elif re.match(r"Thread\s+\d+\s+Crashed:", line):
            crashed = True
        elif re.match(r"Thread\s+\d+\s+", line):
            if crashed:
                break
        elif crashed:
            match = OLD_FRAME.match(line)
            if not match:
                if frames and line.strip():
                    break
                continue
            frame_index = int(match.group(1))
            image = image_name(match.group(2))
            symbol = safe_token(match.group(3).split(" + ", 1)[0])
            item: dict[str, Any] = {"frame": frame_index}
            if image:
                item["image"] = image
            if symbol:
                item["symbol"] = symbol
            if len(item) > 1:
                frames.append(item)
                if len(frames) >= MAX_FRAMES:
                    break

    process_name = image_name(process)
    worker_process = is_worker_process(process_name)
    related_process = bool(
        process_name
        and ("glass" in process_name.casefold() or "native" in process_name.casefold())
    )
    if not worker_process and not related_process:
        return None
    summary: dict[str, Any] = {
        "capture": "matched" if worker_process else "related-process-report",
        "process": WORKER_PROCESS_NAME if worker_process else process_name,
        "frames": frames,
    }
    if exception:
        summary["exception"] = exception
    return summary


def parse_report(path: Path, *, since: float) -> dict[str, Any] | None:
    try:
        metadata = path.lstat()
        if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
            return None
        if metadata.st_mtime < since or metadata.st_size <= 0 or metadata.st_size > MAX_REPORT_BYTES:
            return None
        with path.open("rb") as report:
            raw = report.read(MAX_REPORT_BYTES + 1)
    except OSError:
        return None
    if len(raw) > MAX_REPORT_BYTES:
        return None
    text = raw.decode("utf-8", errors="replace")
    if path.suffix.lower() == ".ips":
        payload = ips_payload(text)
        return ips_summary(payload) if payload else None
    return old_crash_summary(text)


def candidate_reports(since: float, directories: list[Path]) -> list[Path]:
    candidates: list[tuple[float, Path]] = []
    for directory in directories:
        try:
            if directory.is_symlink() or not directory.is_dir():
                continue
            for path in directory.iterdir():
                try:
                    metadata = path.lstat()
                except OSError:
                    continue
                if (
                    path.suffix.lower() not in {".ips", ".crash"}
                    or not stat.S_ISREG(metadata.st_mode)
                    or metadata.st_mtime < since
                ):
                    continue
                candidates.append((metadata.st_mtime, path))
        except OSError:
            continue
    candidates.sort(key=lambda item: item[0], reverse=True)
    return [path for _, path in candidates[:MAX_CANDIDATES]]


def collect_summary(since: float, directories: list[Path], wait_seconds: float) -> dict[str, Any]:
    deadline = time.monotonic() + max(0.0, min(wait_seconds, 20.0))
    recent_reports = 0
    partial_worker_header: dict[str, Any] | None = None
    while True:
        candidates = candidate_reports(since, directories)
        recent_reports = len(candidates)
        process_reports: list[dict[str, str]] = []
        for path in candidates:
            summary = parse_report(path, since=since)
            if summary:
                shape = summary.get("reportShape")
                if (
                    summary.get("capture") == "matched"
                    and isinstance(shape, dict)
                    and shape.get("crashDataPresent") is False
                ):
                    # macOS can create the `.ips` metadata line before it has
                    # written the pretty-printed exception and thread report.
                    # Keep polling for the body instead of treating that
                    # transient header as a complete crash capture.
                    partial_worker_header = summary
                    continue
                return summary
            if path.suffix.lower() == ".ips":
                for report in ips_process_reports_at(path, since=since):
                    if report not in process_reports:
                        process_reports.append(report)
        if process_reports:
            return {
                "capture": "unmatched-process-reports",
                "recentReports": recent_reports,
                "processReports": process_reports[:8],
                "frames": [],
            }
        if time.monotonic() >= deadline:
            if partial_worker_header is not None:
                partial_worker_header["capture"] = "matched-process-header-incomplete"
                partial_worker_header["recentReports"] = recent_reports
                return partial_worker_header
            return {
                "capture": "no-matching-worker-report",
                "recentReports": recent_reports,
                "frames": [],
            }
        time.sleep(0.5)


def self_test() -> None:
    with tempfile.TemporaryDirectory(prefix="glass-macos-crash-test-") as directory:
        root = Path(directory)
        since = time.time() - 2
        ips = root / "glass-native-content-worker-test.ips"
        ips.write_text(
            "\n".join(
                [
                    json.dumps({"app_name": "ignored-private-metadata"}),
                    json.dumps(
                        {
                            "procName": "glass-native-content-worker",
                            "procPath": "/private/tmp/secret-profile/worker",
                            "processArguments": ["secret-page-title"],
                            "exception": {"type": "EXC_CRASH", "signal": "SIGABRT"},
                            "faultingThread": 0,
                            "usedImages": [{"name": "/usr/lib/libsystem_kernel.dylib"}],
                            "threads": [
                                {
                                    "frames": [
                                        {"imageIndex": 0, "symbol": "__pthread_kill", "symbolLocation": 4},
                                        {"imageIndex": 0, "symbol": "https://secret.example/?token=x"},
                                    ]
                                }
                            ],
                        }
                    ),
                ]
            ),
            encoding="utf-8",
        )
        summary = parse_report(ips, since=since)
        assert summary is not None and summary["capture"] == "matched"
        rendered = json.dumps(summary, sort_keys=True)
        assert "__pthread_kill" in rendered
        assert "secret" not in rendered and "https://" not in rendered
        assert "/private/" not in rendered and "token" not in rendered

        truncated = root / "glass-native-content-worker-truncated.ips"
        truncated.write_text(
            json.dumps(
                {
                    "procName": "glass-native-con",
                    "procPath": "/private/tmp/runner/glass-native-content-worker",
                    "exception": {"type": "EXC_CRASH", "signal": "SIGABRT"},
                    "threads": [{"triggered": True, "frames": []}],
                }
            ),
            encoding="utf-8",
        )
        summary = parse_report(truncated, since=since)
        assert summary is not None and summary["process"] == WORKER_PROCESS_NAME
        assert "/private/" not in json.dumps(summary)

        metadata_only = root / "glass-native-content-worker-metadata-only.ips"
        metadata_only.write_text(
            json.dumps(
                {
                    "procName": "glass-native-content-worker",
                    "procPath": "/private/tmp/runner/worker",
                    "processArguments": ["secret-page-title"],
                }
            ),
            encoding="utf-8",
        )
        summary = parse_report(metadata_only, since=since)
        assert summary is not None and summary["frames"] == []
        assert summary["reportShape"] == {
            "exceptionPresent": False,
            "terminationPresent": False,
            "faultingThreadPresent": False,
            "threadsPresent": False,
            "threadCount": 0,
            "usedImagesPresent": False,
            "usedImageCount": 0,
            "crashDataPresent": False,
        }
        assert "/private/" not in json.dumps(summary)
        assert "secret" not in json.dumps(summary)

        growing_directory = root / "growing-report"
        growing_directory.mkdir()
        growing = growing_directory / "glass-native-content-worker-growing.ips"
        growing.write_text(
            json.dumps({"procName": "glass-native-content-worker"}),
            encoding="utf-8",
        )

        def finish_growing_report() -> None:
            time.sleep(0.1)
            with growing.open("a", encoding="utf-8") as report:
                report.write(
                    "\n"
                    + json.dumps(
                        {
                            "exception": {"type": "EXC_CRASH", "signal": "SIGABRT"},
                            "faultingThread": 0,
                            "usedImages": [{"name": "libsystem_kernel.dylib"}],
                            "threads": [
                                {
                                    "frames": [
                                        {"imageIndex": 0, "symbol": "__pthread_kill"}
                                    ]
                                }
                            ],
                        }
                    )
                )

        writer = threading.Thread(target=finish_growing_report, daemon=True)
        writer.start()
        summary = collect_summary(since, [growing_directory], wait_seconds=1)
        writer.join(timeout=1)
        assert not writer.is_alive()
        assert summary["capture"] == "matched"
        assert summary["exception"] == "EXC_CRASH"
        assert summary["frames"] == [
            {"frame": 0, "image": "libsystem_kernel.dylib", "symbol": "__pthread_kill"}
        ]

        pretty_report = root / "glass-native-content-worker-pretty.ips"
        pretty_report.write_text(
            json.dumps({"app_name": "glass-native-content-worker"})
            + "\n"
            + json.dumps(
                {
                    "exception": {"type": "EXC_CRASH", "signal": "SIGABRT"},
                    "threads": [
                        {
                            "triggered": True,
                            "frames": [
                                {"imageIndex": 0, "symbol": "__pthread_kill"},
                            ],
                        }
                    ],
                    "usedImages": [{"name": "/usr/lib/libsystem_kernel.dylib"}],
                },
                indent=2,
            ),
            encoding="utf-8",
        )
        summary = parse_report(pretty_report, since=since)
        assert summary is not None and summary["capture"] == "matched"
        assert summary["frames"] == [
            {"frame": 0, "image": "libsystem_kernel.dylib", "symbol": "__pthread_kill"}
        ]

        path_match = root / "glass-native-content-worker-path.ips"
        path_match.write_text(
            json.dumps(
                {
                    "procName": "renamed-process",
                    "procPath": "/private/tmp/runner/glass-native-content-worker",
                    "threads": [],
                }
            ),
            encoding="utf-8",
        )
        summary = parse_report(path_match, since=since)
        assert summary is not None and summary["process"] == WORKER_PROCESS_NAME
        assert "/private/" not in json.dumps(summary)

        alias = root / "glass-native-worker-alias.ips"
        alias.write_text(
            json.dumps(
                {
                    "procName": "glass_native_worker",
                    "threads": [{"triggered": True, "frames": []}],
                }
            ),
            encoding="utf-8",
        )
        summary = parse_report(alias, since=since)
        assert summary is not None and summary["process"] == WORKER_PROCESS_NAME

        related = root / "glass-related-process.ips"
        related.write_text(
            json.dumps(
                {
                    "procName": "GlassNativeWorkerHost",
                    "procPath": "/private/tmp/secret-profile/GlassNativeWorkerHost",
                    "processArguments": ["private-page-title"],
                    "exception": {"type": "EXC_CRASH", "signal": "SIGABRT"},
                    "faultingThread": 0,
                    "usedImages": [{"name": "/usr/lib/libsystem_kernel.dylib"}],
                    "threads": [
                        {
                            "frames": [
                                {
                                    "imageIndex": 0,
                                    "symbol": "__pthread_kill",
                                    "symbolLocation": 4,
                                }
                            ]
                        }
                    ],
                }
            ),
            encoding="utf-8",
        )
        summary = parse_report(related, since=since)
        assert summary is not None and summary["capture"] == "related-process-report"
        rendered = json.dumps(summary, sort_keys=True)
        assert "GlassNativeWorkerHost" in rendered and "__pthread_kill" in rendered
        assert "secret" not in rendered and "private-page-title" not in rendered
        assert "/private/" not in rendered and "SIGABRT" in rendered

        unrelated = json.dumps(
            {
                "procName": "XCTest",
                "procPath": "/private/tmp/secret-profile/XCTest",
                "processArguments": ["private-page-title"],
                "exception": {"type": "EXC_CRASH", "signal": "SIGABRT"},
            }
        )
        process_reports = ips_process_reports(unrelated)
        rendered = json.dumps(process_reports, sort_keys=True)
        assert process_reports == [
            {"process": "XCTest", "exception": "EXC_CRASH", "signal": "SIGABRT"}
        ]
        assert "secret" not in rendered and "private-page-title" not in rendered
        assert "/private/" not in rendered

        crash = root / "glass-native-content-worker-test.crash"
        crash.write_text(
            "Process: glass-native-content-worker [42]\n"
            "Exception Type: EXC_CRASH (SIGABRT)\n"
            "Thread 0 Crashed:\n"
            "0   libsystem_kernel.dylib 0x0000000100000000 __pthread_kill + 4\n"
            "1   glass-native-content-worker 0x0000000100000001 _main + 8\n\n"
            "Binary Images:\n",
            encoding="utf-8",
        )
        summary = parse_report(crash, since=since)
        assert summary is not None and len(summary["frames"]) == 2
        assert "/private/" not in json.dumps(summary)

        path_crash = root / "glass-native-content-worker-path.crash"
        path_crash.write_text(
            "Process: /private/tmp/runner/glass-native-content-worker [42]\n"
            "Exception Type: EXC_CRASH (SIGABRT)\n",
            encoding="utf-8",
        )
        summary = parse_report(path_crash, since=since)
        assert summary is not None and summary["process"] == WORKER_PROCESS_NAME
        assert "/private/" not in json.dumps(summary)

        related_crash = root / "glass-related-process.crash"
        related_crash.write_text(
            "Process: /private/tmp/secret-profile/GlassNativeWorkerHost [42]\n"
            "Exception Type: EXC_CRASH (SIGABRT)\n"
            "Thread 0 Crashed:\n"
            "0   libsystem_kernel.dylib 0x0000000100000000 __pthread_kill + 4\n",
            encoding="utf-8",
        )
        summary = parse_report(related_crash, since=since)
        assert summary is not None and summary["capture"] == "related-process-report"
        rendered = json.dumps(summary, sort_keys=True)
        assert "GlassNativeWorkerHost" in rendered and "__pthread_kill" in rendered
        assert "secret" not in rendered and "/private/" not in rendered


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--since-file", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--wait-seconds", type=float, default=10.0)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        self_test()
        print("macOS crash summary sanitizer self-test passed")
        return 0
    if args.output is None:
        parser.error("--output is required")

    try:
        since = float(args.since_file.read_text(encoding="ascii").strip()) if args.since_file else time.time() - 900
    except (OSError, ValueError):
        since = time.time() - 900
    directories = [
        Path.home() / "Library/Logs/DiagnosticReports",
        Path("/Library/Logs/DiagnosticReports"),
    ]
    summary = collect_summary(since, directories, args.wait_seconds)
    encoded = (json.dumps(summary, sort_keys=True, indent=2) + "\n").encode("utf-8")
    if len(encoded) > MAX_OUTPUT_BYTES:
        summary = {"capture": "summary-limit-exceeded", "frames": []}
        encoded = (json.dumps(summary, sort_keys=True) + "\n").encode("utf-8")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(encoded)
    print(f"sanitized macOS worker crash summary: {summary['capture']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
