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

    def test_required_tools_abort_ci_on_hash_mismatch(self):
        for tool in ["DENY", "AUDIT"]:
            with self.subTest(tool=tool), tempfile.TemporaryDirectory() as scratch:
                root = Path(scratch)
                archive, digest = self.archive(root)
                extra = '''
tool_version_matches() { [[ "$1" != "${CARGO_''' + tool + '''_VERSION#v}" ]]; }
ensure_tools
echo UNEXPECTED_GATE >&2
exit 0
'''
                result = self.run_installer(root, archive, digest, extra)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("SHA-256 mismatch", result.stderr)
                self.assertNotIn("UNEXPECTED_GATE", result.stderr)


class SmokePreparationTests(unittest.TestCase):
    def run_shell(self, root, body, flags=""):
        target = root / "custom target"
        binary = target / "core-release/glass-core"
        binary.parent.mkdir(parents=True)
        binary.write_text("fresh")
        binary.chmod(0o755)
        override = root / "old-core"
        override.write_text("old")
        override.chmod(0o755)
        return subprocess.run(
            ["bash", "-c", '''source "$TEST_SCRIPT" ''' + flags + '''
cargo() {
  if [[ "$1" == metadata ]]; then
    printf '{"target_directory":"%s"}\\n' "$TEST_TARGET"
  else
    printf '%s\\n' "$*" >> "$TEST_LOG"
  fi
}
''' + body],
            env=dict(os.environ, TEST_SCRIPT=str(SCRIPT), TEST_TARGET=str(target),
                     TEST_LOG=str(root / "calls"), GLASS_CORE_TEST_BINARY=str(override)),
            capture_output=True, text=True,
        )

    def test_default_builds_and_uses_metadata_target_not_override(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            result = self.run_shell(root, "prepare_core_binary")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), str(root / "custom target/core-release/glass-core"))
            self.assertIn("build --profile core-release --locked --package glass-core --bin glass-core", (root / "calls").read_text())

    def test_reuse_is_explicit_and_build_flag_overrides_it(self):
        for flags, builds in [("--reuse-core", False), ("--reuse-core --build-core", True)]:
            with self.subTest(flags=flags), tempfile.TemporaryDirectory() as scratch:
                root = Path(scratch)
                result = self.run_shell(root, "prepare_core_binary", flags)
                self.assertEqual(result.returncode, 0, result.stderr)
                expected = root / ("custom target/core-release/glass-core" if builds else "old-core")
                self.assertEqual(result.stdout.strip(), str(expected))
                self.assertEqual((root / "calls").exists(), builds)

    def test_failed_build_never_falls_back_to_an_old_binary(self):
        with tempfile.TemporaryDirectory() as scratch:
            result = self.run_shell(Path(scratch), "cargo() { return 9; }; prepare_core_binary")
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")

    def test_missing_binary_does_not_reuse_override_after_build(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            result = self.run_shell(root, 'rm "$TEST_TARGET/core-release/glass-core"; prepare_core_binary')
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")

    def test_smoke_fixtures_are_installed_and_built(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            result = self.run_shell(root, '''python3() { return 0; }
npm() { printf '%s\\n' "$*" >> "$TEST_LOG"; }
prepare_smoke_fixtures''')
            self.assertEqual(result.returncode, 0, result.stderr)
            calls = (root / "calls").read_text().splitlines()
            self.assertEqual(len(calls), 2)
            self.assertTrue(calls[0].endswith("ci --ignore-scripts"))
            self.assertTrue(calls[1].endswith("run build"))

    def test_missing_pyte_fails_before_building_fixtures(self):
        with tempfile.TemporaryDirectory() as scratch:
            root = Path(scratch)
            result = self.run_shell(root, "python3() { return 1; }; prepare_smoke_fixtures")
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("python3 -m pip install pyte==0.8.2", result.stderr)
            self.assertFalse((root / "calls").exists())


class WorkflowTests(unittest.TestCase):
    def test_old_release_tags_do_not_supply_the_setup_action(self):
        workflow = (SCRIPT.parents[1] / ".github/workflows/release-native-certification.yml").read_text()
        self.assertNotIn("uses: ./.github/actions/setup-ci-rust", workflow)
        self.assertEqual(workflow.count("uses: dtolnay/rust-toolchain@df2a88e9fe4098bad09cb42c3dec7187f15861fc"), 2)
        self.assertEqual(workflow.count("ref: ${{ inputs.target_ref }}"), 2)
        self.assertEqual(workflow.count('test "${actual_sha}" = "${EXPECTED_SHA}"'), 2)

    def test_fuzz_cache_is_relative_to_its_workspace(self):
        workflow = (SCRIPT.parents[1] / ".github/workflows/fuzz.yml").read_text()
        self.assertIn("workspaces: fuzz -> target", workflow)
        self.assertNotIn("fuzz -> fuzz/target", workflow)


if __name__ == "__main__":
    unittest.main()
