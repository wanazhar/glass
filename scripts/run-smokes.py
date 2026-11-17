#!/usr/bin/env python3
"""Run Glass Core smoke scripts, optionally in parallel.

Scripts known to share a fixed port or to mutate a global Cargo/install
tree are serialized. Every other script may run concurrently. The process
exits non-zero if any script fails, and the failed script's output is
printed in full.
"""

from __future__ import annotations

import argparse
import os
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

# Basenames that must not overlap with other smokes (fixed bind or cargo install).
DEFAULT_SERIAL = frozenset(
    {
        "smoke-connect.py",
        "smoke-connect-lifecycle.py",
        "smoke-upgrade.py",
    }
)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "scripts",
        nargs="*",
        type=Path,
        help="Smoke scripts to run (also accepted via --manifest)",
    )
    parser.add_argument(
        "--manifest",
        type=Path,
        help="File with one script path per line (comments starting with # ignored)",
    )
    parser.add_argument(
        "--jobs",
        type=int,
        default=0,
        help="Parallel workers for safe scripts (default: CPU count, minimum 1)",
    )
    parser.add_argument(
        "--serial",
        action="append",
        default=[],
        metavar="NAME",
        help="Additional script basename to serialize (repeatable)",
    )
    parser.add_argument(
        "--timeout",
        type=int,
        default=0,
        help="Per-script timeout in seconds (0 disables)",
    )
    parser.add_argument(
        "--save-stdout",
        action="append",
        default=[],
        metavar="SCRIPT=FILE",
        help="Write a script's stdout to FILE (path is relative to the cwd)",
    )
    parser.add_argument(
        "--cwd",
        type=Path,
        default=None,
        help="Working directory for scripts (default: current directory)",
    )
    return parser.parse_args(argv)


def load_manifest(path: Path) -> list[Path]:
    scripts: list[Path] = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        scripts.append(Path(line))
    return scripts


def parse_save_stdout(values: list[str]) -> dict[str, Path]:
    mapping: dict[str, Path] = {}
    for item in values:
        if "=" not in item:
            raise SystemExit(f"--save-stdout expects SCRIPT=FILE, got {item!r}")
        script, dest = item.split("=", 1)
        mapping[Path(script).name] = Path(dest)
        mapping[script] = Path(dest)
    return mapping


def partition(scripts: list[Path], serial_names: set[str]) -> tuple[list[Path], list[Path]]:
    parallel: list[Path] = []
    serial: list[Path] = []
    for script in scripts:
        if script.name in serial_names:
            serial.append(script)
        else:
            parallel.append(script)
    return parallel, serial


def run_one(
    script: Path,
    cwd: Path,
    timeout: int | None,
    save_stdout: dict[str, Path],
) -> dict:
    started = time.perf_counter()
    try:
        completed = subprocess.run(
            [sys.executable, str(script)],
            cwd=cwd,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
        duration = time.perf_counter() - started
        stdout = completed.stdout
        stderr = completed.stderr
        returncode = completed.returncode
        error = None
    except subprocess.TimeoutExpired as exc:
        duration = time.perf_counter() - started
        stdout = exc.stdout.decode() if isinstance(exc.stdout, bytes) else (exc.stdout or "")
        stderr = exc.stderr.decode() if isinstance(exc.stderr, bytes) else (exc.stderr or "")
        returncode = 124
        error = f"timed out after {timeout}s"
    dest = save_stdout.get(script.name) or save_stdout.get(str(script))
    if dest is not None:
        dest_path = dest if dest.is_absolute() else cwd / dest
        dest_path.parent.mkdir(parents=True, exist_ok=True)
        dest_path.write_text(stdout, encoding="utf-8")
    return {
        "script": script,
        "returncode": returncode,
        "stdout": stdout,
        "stderr": stderr,
        "duration": duration,
        "error": error,
    }


def display_name(script: Path, cwd: Path) -> str:
    try:
        return str(script.resolve().relative_to(cwd))
    except ValueError:
        return str(script)


def report(result: dict, cwd: Path) -> None:
    script = display_name(result["script"], cwd)
    duration = result["duration"]
    if result["returncode"] == 0 and not result["error"]:
        print(f"PASS  {script}  ({duration:.1f}s)", flush=True)
        return
    print(f"FAIL  {script}  ({duration:.1f}s) exit {result['returncode']}", flush=True)
    if result["error"]:
        print(result["error"], flush=True)
    if result["stdout"]:
        print("----- stdout -----", flush=True)
        print(result["stdout"], end="" if result["stdout"].endswith("\n") else "\n", flush=True)
    if result["stderr"]:
        print("----- stderr -----", flush=True)
        print(result["stderr"], end="" if result["stderr"].endswith("\n") else "\n", flush=True)


def run_serial(scripts: list[Path], cwd: Path, timeout: int | None, save_stdout: dict[str, Path]) -> list[dict]:
    results = []
    for script in scripts:
        result = run_one(script, cwd, timeout, save_stdout)
        report(result, cwd)
        results.append(result)
    return results


def run_parallel(
    scripts: list[Path],
    cwd: Path,
    jobs: int,
    timeout: int | None,
    save_stdout: dict[str, Path],
) -> list[dict]:
    if not scripts:
        return []
    if jobs <= 1:
        return run_serial(scripts, cwd, timeout, save_stdout)
    results: list[dict] = []
    with ThreadPoolExecutor(max_workers=jobs) as pool:
        futures = {
            pool.submit(run_one, script, cwd, timeout, save_stdout): script for script in scripts
        }
        for future in as_completed(futures):
            result = future.result()
            report(result, cwd)
            results.append(result)
    return results


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    scripts = list(args.scripts)
    if args.manifest:
        scripts.extend(load_manifest(args.manifest))
    if not scripts:
        print("no smoke scripts given", file=sys.stderr)
        return 2
    cwd = (args.cwd or Path.cwd()).resolve()
    resolved: list[Path] = []
    for script in scripts:
        path = script if script.is_absolute() else cwd / script
        resolved.append(path)
    missing = [orig for orig, path in zip(scripts, resolved) if not path.is_file()]
    if missing:
        for script in missing:
            print(f"missing smoke script: {script}", file=sys.stderr)
        return 2
    scripts = resolved
    jobs = args.jobs if args.jobs > 0 else (os.cpu_count() or 1)
    jobs = max(1, jobs)
    timeout = args.timeout if args.timeout > 0 else None
    save_stdout = parse_save_stdout(args.save_stdout)
    serial_names = set(DEFAULT_SERIAL)
    serial_names.update(args.serial)
    parallel, serial = partition(scripts, serial_names)
    print(
        f"running {len(parallel)} smoke(s) with {jobs} worker(s), "
        f"{len(serial)} serialized",
        flush=True,
    )
    results = run_parallel(parallel, cwd, jobs, timeout, save_stdout)
    results.extend(run_serial(serial, cwd, timeout, save_stdout))
    failed = [result for result in results if result["returncode"] != 0 or result["error"]]
    print(
        f"completed {len(results)} smoke(s): {len(results) - len(failed)} passed, "
        f"{len(failed)} failed",
        flush=True,
    )
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
