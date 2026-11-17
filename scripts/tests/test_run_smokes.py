#!/usr/bin/env python3
"""Unit tests for scripts/run-smokes.py."""

import os
import stat
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
RUNNER = ROOT / "run-smokes.py"


def write_script(directory: Path, name: str, body: str) -> Path:
    path = directory / name
    path.write_text("#!/usr/bin/env python3\n" + body, encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IEXEC)
    return path


class RunSmokesTests(unittest.TestCase):
    def run_runner(self, args, cwd):
        return subprocess.run(
            [sys.executable, str(RUNNER), *args],
            cwd=cwd,
            capture_output=True,
            text=True,
        )

    def test_parallel_scripts_pass_and_save_stdout(self):
        with tempfile.TemporaryDirectory(prefix="run-smokes-") as scratch:
            root = Path(scratch)
            write_script(root, "a.py", "import time\ntime.sleep(0.2)\nprint('alpha')\n")
            write_script(root, "b.py", "import time\ntime.sleep(0.2)\nprint('beta')\n")
            started = time.perf_counter()
            result = self.run_runner(
                ["--jobs", "2", "--save-stdout", "a.py=out-a.txt", "a.py", "b.py"],
                cwd=root,
            )
            elapsed = time.perf_counter() - started
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("2 passed", result.stdout)
            self.assertLess(elapsed, 1.0)
            self.assertEqual((root / "out-a.txt").read_text(encoding="utf-8"), "alpha\n")

    def test_failure_prints_output_and_is_nonzero(self):
        with tempfile.TemporaryDirectory(prefix="run-smokes-") as scratch:
            root = Path(scratch)
            write_script(root, "ok.py", "print('ok')\n")
            write_script(root, "bad.py", "import sys\nprint('boom-out')\nprint('boom-err', file=sys.stderr)\nsys.exit(7)\n")
            result = self.run_runner(["--jobs", "2", "ok.py", "bad.py"], cwd=root)
            self.assertEqual(result.returncode, 1)
            self.assertIn("FAIL  bad.py", result.stdout)
            self.assertIn("boom-out", result.stdout)
            self.assertIn("boom-err", result.stdout)
            self.assertIn("1 failed", result.stdout)

    def test_serial_scripts_do_not_overlap(self):
        with tempfile.TemporaryDirectory(prefix="run-smokes-") as scratch:
            root = Path(scratch)
            lock = root / "lock"
            body = f"""
import os, time
from pathlib import Path
lock = Path({str(lock)!r})
if lock.exists():
    raise SystemExit('overlap')
lock.write_text('held')
time.sleep(0.2)
lock.unlink()
print('done')
"""
            write_script(root, "one.py", body)
            write_script(root, "two.py", body)
            result = self.run_runner(
                ["--jobs", "2", "--serial", "one.py", "--serial", "two.py", "one.py", "two.py"],
                cwd=root,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("2 serialized", result.stdout)

    def test_cwd_resolves_manifest_paths(self):
        with tempfile.TemporaryDirectory(prefix="run-smokes-") as scratch:
            root = Path(scratch)
            product = root / "products" / "core"
            scripts = product / "scripts"
            scripts.mkdir(parents=True)
            write_script(scripts, "ok.py", "print('ok')\n")
            manifest = scripts / "list.txt"
            manifest.write_text("scripts/ok.py\n", encoding="utf-8")
            result = self.run_runner(
                ["--cwd", str(product), "--manifest", str(manifest), "--jobs", "1"],
                cwd=root,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("1 passed", result.stdout)

    def test_manifest_and_missing_script(self):
        with tempfile.TemporaryDirectory(prefix="run-smokes-") as scratch:
            root = Path(scratch)
            write_script(root, "ok.py", "print('ok')\n")
            manifest = root / "list.txt"
            manifest.write_text("ok.py\n# comment\nmissing.py\n", encoding="utf-8")
            result = self.run_runner(["--manifest", str(manifest)], cwd=root)
            self.assertEqual(result.returncode, 2)
            self.assertIn("missing smoke script: missing.py", result.stderr)


if __name__ == "__main__":
    unittest.main()
