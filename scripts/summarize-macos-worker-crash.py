#!/usr/bin/env python3
"""Write a bounded, privacy-safe summary of a macOS native-worker crash."""

from __future__ import annotations

import argparse
import json
import re
import stat
import sys
import tempfile
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


def is_worker_process(value: Any) -> bool:
    if not isinstance(value, str):
        return False
    value = value.replace("\\", "/").rsplit("/", 1)[-1]
    if value == WORKER_PROCESS_NAME:
        return True
    # macOS may expose a 16-character procName even though the executable's
    # full basename is longer. Accept only this fixed, distinctive prefix.
    return (
        len(WORKER_PROCESS_PREFIX) <= len(value) <= 20
        and WORKER_PROCESS_NAME.startswith(value)
    )


def ips_payload(text: str) -> dict[str, Any] | None:
    candidates: list[dict[str, Any]] = []
    for line in text.splitlines()[:64]:
        try:
            value = json.loads(line)
        except (json.JSONDecodeError, TypeError):
            continue
        if isinstance(value, dict):
            candidates.append(value)
    if not candidates:
        try:
            value = json.loads(text)
        except (json.JSONDecodeError, TypeError):
            return None
        if isinstance(value, dict):
            candidates.append(value)
    return next(
        (
            value
            for value in reversed(candidates)
            if is_worker_process(value.get("procName"))
            or is_worker_process(value.get("procPath"))
        ),
        None,
    )


def ips_summary(payload: dict[str, Any]) -> dict[str, Any]:
    summary: dict[str, Any] = {
        "capture": "matched",
        "process": WORKER_PROCESS_NAME,
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
                process = safe_token(match.group(1), limit=100)
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

    if not is_worker_process(process):
        return None
    summary: dict[str, Any] = {
        "capture": "matched",
        "process": WORKER_PROCESS_NAME,
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
    while True:
        candidates = candidate_reports(since, directories)
        recent_reports = len(candidates)
        for path in candidates:
            summary = parse_report(path, since=since)
            if summary:
                return summary
        if time.monotonic() >= deadline:
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
