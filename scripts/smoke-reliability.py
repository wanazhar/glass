#!/usr/bin/env python3
"""Offline regression gate for durable session and turn reliability."""
import subprocess
import sys
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

def run(*args):
    print("+", " ".join(args), flush=True)
    subprocess.run(args, cwd=ROOT, check=True)

def main():
    configured_binary = os.environ.get("GLASS_CORE_TEST_BINARY")
    if not configured_binary or not Path(configured_binary).is_file():
        run("cargo", "build", "-p", "glass-core", "--bin", "glass-core")
    for filter_ in ("session::tests", "partial::tests", "serve::tests"):
        run("cargo", "test", "-p", "glass-core", filter_)
    run(sys.executable, "scripts/smoke-recovery.py")
    run(sys.executable, "scripts/smoke-serve.py")

if __name__ == "__main__":
    main()
