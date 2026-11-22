import importlib.util
import json
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
            result = self.run_task(workspace, '{"text":"ok"}\n')
            self.assertTrue(result["success"])
            self.assertEqual(keep.read_text(), "human-owned")
            self.assertEqual(nested.read_text(), "source")
            json.dumps(result)

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
