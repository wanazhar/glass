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
            runner.write_eval_outputs(workspace, [owned])
            manifest = workspace / ".browser-cu-outputs.json"
            records = json.loads(manifest.read_text())
            records += [{"path": str(external), "identity": runner.output_identity(external)},
                        {"path": "escape/keep.txt", "identity": runner.output_identity(external)}]
            manifest.write_text(json.dumps(records))
            runner.clear_eval_outputs(workspace)
            self.assertFalse(owned.exists())
            self.assertTrue(unrelated.exists())
            self.assertTrue(external.exists())
            self.assertTrue(recordings.is_dir())
            runner.clear_eval_outputs(workspace)

    def test_manifest_symlink_never_overwrites_its_target(self):
        with tempfile.TemporaryDirectory() as root, tempfile.TemporaryDirectory() as outside:
            workspace = Path(root)
            target = Path(outside) / "private"
            target.write_text("keep")
            manifest = workspace / ".browser-cu-outputs.json"
            manifest.symlink_to(target)
            runner.clear_eval_outputs(workspace)
            runner.write_eval_outputs(workspace, [])
            self.assertEqual(target.read_text(), "keep")
            self.assertFalse(manifest.is_symlink())
            self.assertEqual(json.loads(manifest.read_text()), [])

    def test_cleanup_keeps_replaced_and_modified_outputs(self):
        with tempfile.TemporaryDirectory() as root:
            workspace = Path(root)
            output = workspace / "output.mp4"
            output.write_text("eval")
            runner.write_eval_outputs(workspace, [output])
            output.write_text("human replacement")
            runner.clear_eval_outputs(workspace)
            self.assertEqual(output.read_text(), "human replacement")
            runner.write_eval_outputs(workspace, [output])
            output.unlink()
            output.write_text("new inode")
            runner.clear_eval_outputs(workspace)
            self.assertEqual(output.read_text(), "new inode")

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
