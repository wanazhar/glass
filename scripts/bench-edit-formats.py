#!/usr/bin/env python3
"""End-to-end native revision edits vs the bundled hashline implementation.

Both paths start a fresh process per operation. Hashline's worker calls the real
extension functions and commits their proposed text; the worker adds no anchoring
logic. This is a deterministic format/safety benchmark, not a model success-rate
claim. No provider, installed configuration or existing project file is touched.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def worker(args):
    spec = importlib.util.spec_from_file_location("hashline", ROOT / "extensions/hashline/hashline.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    request = json.load(sys.stdin)
    path = request["path"]
    if args.worker == "read":
        lines, first = module.read_lines(path, request["line"], 1)
        result = {"text": module.annotate("\n".join(lines), first)}
    else:
        proposed, _ = module.apply_ops(path, request["ops"])
        # The extension delegates its write to bash; keep both benchmark paths
        # process-inclusive, without hiding newline normalization in the worker.
        Path(path).write_bytes(proposed.encode())
        result = {"bytes": len(proposed.encode())}
    print(json.dumps(result))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default=os.environ.get("GLASS_CORE_TEST_BINARY", str(ROOT / "target/release/glass-core")))
    parser.add_argument("--repeats", type=int, default=3)
    parser.add_argument("--worker", choices=["read", "edit"])
    args = parser.parse_args()
    if args.worker:
        worker(args)
        return
    if not 1 <= args.repeats <= 20:
        parser.error("repeats must be 1–20")
    binary = str(Path(args.binary).resolve(strict=True))
    fixtures = {
        "small": ("let value = 1;\n" * 2000).encode(),
        "large": ("let value = 1;\n" * 75000).encode(),
        "unicode_repeated": ("const 界 = 'é';\n" * 4000).encode(),
        "crlf": b"same line\r\n" * 4000,
    }
    rows = []
    with tempfile.TemporaryDirectory(prefix="gc-edit-bench-") as directory:
        root = Path(directory)
        def invoke(command, request):
            payload = json.dumps(request).encode()
            started = time.perf_counter_ns()
            completed = subprocess.run(command, input=payload, capture_output=True, cwd=root, timeout=30)
            elapsed = (time.perf_counter_ns() - started) / 1e6
            if completed.returncode:
                raise RuntimeError(completed.stderr.decode(errors="replace"))
            return json.loads(completed.stdout), elapsed, len(payload), len(completed.stdout)
        for label, source in fixtures.items():
            line = len(source.splitlines()) // 2
            for kind in ["revision", "hashline"]:
                read_ms, edit_ms = [], []
                request_bytes, response_bytes = [], []
                for _ in range(args.repeats):
                    path = root / "fixture.txt"
                    path.write_bytes(source)
                    if kind == "revision":
                        started = time.perf_counter_ns()
                        read = subprocess.run([binary, "file", "--json", "read", "fixture.txt", str(line), "1"], capture_output=True, cwd=root, timeout=30)
                        read_ms.append((time.perf_counter_ns() - started) / 1e6)
                        if read.returncode:
                            raise RuntimeError(read.stderr.decode(errors="replace"))
                        revision = json.loads(read.stdout)["revision"]
                        request = {"path":"fixture.txt", "expected":revision, "edits":[{"start":line,"end":line,"text":"changed"}]}
                        command = [binary, "file", "--json", "edit"]
                        response_bytes.append(len(read.stdout))
                    else:
                        read, elapsed, _, size = invoke([sys.executable, __file__, "--worker", "read"], {"path":str(path),"line":line})
                        read_ms.append(elapsed)
                        anchor = read["text"].split("|",1)[0]
                        request = {"path":str(path),"ops":[{"op":"replace","line":anchor,"text":"changed"}]}
                        command = [sys.executable, __file__, "--worker", "edit"]
                        response_bytes.append(size)
                    _, elapsed, sent, size = invoke(command, request)
                    edit_ms.append(elapsed); request_bytes.append(sent); response_bytes[-1] += size
                    result = path.read_bytes()
                    expected_lines = source.splitlines(keepends=True)
                    ending = b"\r\n" if source.endswith(b"\r\n") else b"\n"
                    expected_lines[line-1] = b"changed" + ending
                    correct = result == b"".join(expected_lines)
                rows.append({"fixture":label,"format":kind,"source_bytes":len(source),"read_median_ms":statistics.median(read_ms),"edit_median_ms":statistics.median(edit_ms),"edit_request_bytes":statistics.median(request_bytes),"read_and_edit_response_bytes":statistics.median(response_bytes),"exact_expected_bytes":correct,"result_sha256":hashlib.sha256(result).hexdigest()})
    print(json.dumps({"method":"Fresh process per read/edit for both paths; filesystem cache not flushed. Hashline worker uses bundled read_lines/apply_ops; no model success-rate inference.","repeats":args.repeats,"rows":rows}, indent=2))


if __name__ == "__main__":
    main()
