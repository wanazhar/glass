import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("run_eval", Path(__file__).with_name("run_eval.py"))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class ArtifactTests(unittest.TestCase):
    def test_cleanup_only_removes_owned_outputs_and_rejects_escapes(self):
        with tempfile.TemporaryDirectory() as root, tempfile.TemporaryDirectory() as outside:
            workspace = Path(root)
            recordings = workspace / ".gc-artifacts/desktop/recordings"
            recordings.mkdir(parents=True)
            owned = recordings / "eval.mp4"
            unrelated = recordings / "human.mp4"
            external = Path(outside) / "keep.txt"
            for path in [owned, unrelated, external]:
                path.write_text("keep")
            (workspace / "escape").symlink_to(outside, target_is_directory=True)
            (workspace / ".browser-cu-outputs.json").write_text(json.dumps([
                str(owned.relative_to(workspace)), str(external), "escape/keep.txt"]))
            runner.clear_eval_outputs(workspace)
            self.assertFalse(owned.exists())
            self.assertTrue(unrelated.exists())
            self.assertTrue(external.exists())
            self.assertTrue(recordings.is_dir())
            runner.clear_eval_outputs(workspace)

    def test_existing_recordings_do_not_pass_a_failed_run(self):
        with tempfile.TemporaryDirectory() as root:
            workspace = Path(root)
            stale = workspace / "old.mp4"
            stale.write_bytes(b"video")
            task = {"check": {"workspace_file": {"glob": "*.mp4"}}}
            self.assertFalse(runner.check(task, "", [], "", workspace, {stale}))
            (workspace / "new.mp4").write_bytes(b"video")
            self.assertTrue(runner.check(task, "", [], "", workspace, {stale}))


if __name__ == "__main__":
    unittest.main()
