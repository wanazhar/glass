import hashlib
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "package-release-binaries.sh"


class ReleaseArchiveTests(unittest.TestCase):
    def test_checksums_use_lf_even_with_windows_text_translation(self):
        for target in ["x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"]:
            with self.subTest(target=target), tempfile.TemporaryDirectory() as scratch:
                root = Path(scratch)
                bins = root / "bin"
                bins.mkdir()
                suffix = ".exe" if "windows" in target else ""
                for name in ["glass", "glass-browser", "glass-native-content-worker"]:
                    (bins / (name + suffix)).write_bytes(name.encode())
                (root / "sitecustomize.py").write_text('''from pathlib import Path
original = Path.write_text
def windows_write_text(self, data, *args, **kwargs):
    kwargs["newline"] = "\\r\\n"
    return original(self, data, *args, **kwargs)
Path.write_text = windows_write_text
''')
                out = root / "out"
                result = subprocess.run(
                    ["bash", str(SCRIPT), "--target", target, "--bin-dir", str(bins), "--output-dir", str(out)],
                    env=dict(os.environ, PYTHONPATH=str(root)), capture_output=True, text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)
                archive = next(p for p in out.iterdir() if not p.name.endswith(".sha256"))
                checksum = archive.with_name(archive.name + ".sha256")
                expected = f"{hashlib.sha256(archive.read_bytes()).hexdigest()}  {archive.name}\n".encode()
                self.assertEqual(checksum.read_bytes(), expected)
                verified = subprocess.run(["sha256sum", "-c", checksum.name], cwd=out, capture_output=True, text=True)
                self.assertEqual(verified.returncode, 0, verified.stderr)

    def test_missing_and_invalid_inputs_are_rejected(self):
        with tempfile.TemporaryDirectory() as scratch:
            for args in [[], ["--target", "../escape", "--output-dir", scratch],
                         ["--target", "x86_64-pc-windows-msvc", "--bin-dir", scratch, "--output-dir", scratch]]:
                result = subprocess.run(["bash", str(SCRIPT), *args], capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
