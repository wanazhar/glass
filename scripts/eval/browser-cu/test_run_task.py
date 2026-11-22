import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("run_eval", Path(__file__).with_name("run_eval.py"))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class RunTaskTests(unittest.TestCase):
    def run_task(self, workspace, outcome):
        task = {"id": "probe", "family": "browser", "capability": "probe",
                "prompt": "probe {base}", "check": {"answer_contains": ["ok"]}}
        args = SimpleNamespace(glass_core="unused", model=None, timeout=1)
        with patch.object(runner, "reset_fixture"), patch.object(runner.subprocess, "run") as run:
            if isinstance(outcome, Exception):
                run.side_effect = outcome
            else:
                run.return_value = subprocess.CompletedProcess([], 0, outcome, "")
            return runner.run_task(task, {"browser": []}, args, "unused", workspace,
                                   workspace, workspace)

    def test_existing_workspace_files_and_directories_are_preserved(self):
        with tempfile.TemporaryDirectory() as root:
            workspace = Path(root)
            keep = workspace / "human.txt"
            keep.write_text("human-owned")
            nested = workspace / "project" / "source.txt"
            nested.parent.mkdir()
            nested.write_text("source")
            receipt = workspace / "receipt-0921.txt"
            receipt.write_text("real receipt")
            result = self.run_task(workspace, '{"text":"ok"}\n')
            self.assertTrue(result["success"])
            self.assertEqual(keep.read_text(), "human-owned")
            self.assertEqual(nested.read_text(), "source")
            self.assertEqual(receipt.read_text(), "real receipt")
            json.dumps(result)

    def test_browser_wrapper_overrides_both_runtime_flag_forms(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            browser = root / "browser"
            browser.write_text(f"#!{runner.sys.executable}\nimport json, sys\nprint(json.dumps(sys.argv[1:]))\n")
            browser.chmod(0o755)
            wrapper = root / "wrapper"
            runner.write_browser_wrapper(wrapper, browser, "chromium")
            for arguments in [[], ["--browser-runtime", "native"], ["--browser-runtime=native"]]:
                output = subprocess.check_output([str(wrapper), *arguments, "session", "start", "eval"], text=True)
                self.assertEqual(json.loads(output), ["--browser-runtime", "chromium", "session", "start", "eval"])

    def test_desktop_data_dir_preserves_the_manager_default(self):
        with patch.dict(os.environ, {}, clear=True):
            desktop = [{"family":"desktop"}]
            self.assertEqual(runner.eval_data_dir("", desktop, Path("/scratch")), Path.home() / ".local/share")
            self.assertEqual(runner.eval_data_dir(None, [{"family":"browser"}], Path("/scratch")), Path("/scratch/data"))
        with patch.dict(os.environ, {"XDG_DATA_HOME":"/inherited"}):
            self.assertEqual(runner.eval_data_dir(None, desktop, Path("/scratch")), Path("/inherited"))
            self.assertEqual(runner.eval_data_dir("/override", desktop, Path("/scratch")), Path("/override"))

    def test_verifier_execution_errors_are_reported_per_task(self):
        with tempfile.TemporaryDirectory() as root, patch.object(runner, "check", side_effect=FileNotFoundError("xclip")):
            result = self.run_task(Path(root), '{"text":"ok"}')
            self.assertFalse(result["success"])
            self.assertIn("xclip", result["infrastructure_error"])

    def test_empty_output_still_produces_a_serializable_failure(self):
        with tempfile.TemporaryDirectory() as root:
            result = self.run_task(Path(root), "")
            self.assertFalse(result["success"])
            json.dumps(result)

    def test_timeout_byte_output_is_decoded_before_building_the_report(self):
        for output, stderr in [(b'{"text":"partial"}\n', b"deadline\xff"), (None, None)]:
            with self.subTest(output=output), tempfile.TemporaryDirectory() as root:
                result = self.run_task(Path(root), subprocess.TimeoutExpired(
                    "unused", 1, output=output, stderr=stderr))
                self.assertEqual(result["exit"], "timeout")
                self.assertIsInstance(result["stderr_tail"], str)
                json.dumps(result)


if __name__ == "__main__":
    unittest.main()
