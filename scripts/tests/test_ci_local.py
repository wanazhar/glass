"""Regression tests for verified local CI tool installation."""

import hashlib
import io
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "ci-local.sh"


class InstallerTests(unittest.TestCase):
    def run_installer(self, root, archive, checksum, extra=""):
        return subprocess.run(
            [
                "bash", "-c",
                '''script="$1"; checksum="$2"; set --
source "$script"
curl() { cp "$TEST_ARCHIVE" "$4"; }
''' + extra + '''
if install_from_tarball https://example.invalid/pinned.tar.gz "$checksum" test-tool; then
  exit 0
else
  exit 1
fi
''',
                "test", str(SCRIPT), checksum,
            ],
            env=dict(os.environ, CARGO_HOME=str(root / "cargo"), TEST_ARCHIVE=str(archive)),
            capture_output=True, text=True,
        )

    def archive(self, root):
        path = root / "tool.tar.gz"
        with tarfile.open(path, "w:gz") as archive:
            info = tarfile.TarInfo("release/test-tool")
            payload = b"#!/bin/sh\necho verified\n"
            info.size = len(payload)
            archive.addfile(info, io.BytesIO(payload))
        return path, hashlib.sha256(path.read_bytes()).hexdigest()

    def test_verified_archive_installs_executable(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            archive, digest = self.archive(root)
            result = self.run_installer(root, archive, digest)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            binary = root / "cargo/bin/test-tool"
            self.assertTrue(os.access(binary, os.X_OK))
            self.assertEqual(subprocess.check_output([str(binary)], text=True), "verified\n")

    def test_hash_mismatch_never_extracts_or_replaces_existing_tool(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            archive, _ = self.archive(root)
            binary = root / "cargo/bin/test-tool"
            binary.parent.mkdir(parents=True)
            binary.write_text("existing")
            result = self.run_installer(root, archive, "0" * 64, 'tar() { echo UNEXPECTED_EXTRACTION >&2; return 99; }')
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("SHA-256 mismatch", result.stderr)
            self.assertNotIn("UNEXPECTED_EXTRACTION", result.stderr)
            self.assertEqual(binary.read_text(), "existing")

    def test_download_failure_does_not_install(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            archive, digest = self.archive(root)
            result = self.run_installer(root, archive, digest, "curl() { return 22; }")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("failed to download", result.stderr)
            self.assertFalse((root / "cargo/bin/test-tool").exists())

    def test_empty_archive_does_not_install(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            archive = root / "empty.tar.gz"
            archive.write_bytes(b"")
            result = self.run_installer(root, archive, hashlib.sha256(b"").hexdigest())
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((root / "cargo/bin/test-tool").exists())

    def test_extraction_failure_does_not_install_partial_contents(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            archive, digest = self.archive(root)
            result = self.run_installer(root, archive, digest, 'tar() { command tar "$@"; return 1; }')
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse((root / "cargo/bin/test-tool").exists())


if __name__ == "__main__":
    unittest.main()
