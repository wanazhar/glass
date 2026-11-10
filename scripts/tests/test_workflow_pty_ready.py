import importlib.util
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / "products" / "glass-core" / "scripts"
sys.path.insert(0, str(SCRIPTS))

workflow_spec = importlib.util.spec_from_file_location(
    "smoke_workflow_next", SCRIPTS / "smoke-workflow-next.py"
)
workflow = importlib.util.module_from_spec(workflow_spec)
workflow_spec.loader.exec_module(workflow)

features_spec = importlib.util.spec_from_file_location(
    "smoke_features", SCRIPTS / "smoke-features.py"
)
features = importlib.util.module_from_spec(features_spec)
features_spec.loader.exec_module(features)


CI_20X8 = (
    "Work                \n"
    "Find:               \n"
    "› Task 1 · done · pa\n"
    "  Task 2 · claimed ·\n"
    "  Task 3 · pending ·\n"
    "  /desktop · Choose \n"
    "  /todo · Inspect an\n"
    "↵ · Esc · → details "
)
PALETTE_WORK_ROW = (
    "Commands            \n"
    "❯ /work             \n"
    "› Work              \n"
    "  guidance          \n"
    "Enter · Esc close   "
)
WIDE_WORK = (
    "Work\n"
    "Current project / conversation · selected details load on demand\n"
    "Search:\n"
    "› Task 1 · pending · parser\n"
)


class WorkflowPtyReadyTests(unittest.TestCase):
    def test_accepts_compact_work_chooser_from_ci(self):
        self.assertTrue(workflow.workspace_overlay_ready(CI_20X8))
        self.assertNotIn("Workspace", CI_20X8)

    def test_rejects_palette_work_row_before_submit(self):
        self.assertFalse(workflow.workspace_overlay_ready(PALETTE_WORK_ROW))

    def test_accepts_wide_work_chooser(self):
        self.assertTrue(workflow.workspace_overlay_ready(WIDE_WORK))

    def test_quit_dialog_matches_rendered_confirmation(self):
        self.assertTrue(features.quit_dialog_visible("╭ EXIT · confirm ──╮\n│ QUIT?            │"))
        self.assertTrue(features.quit_dialog_visible("Quit?\n↵ exit"))
        self.assertFalse(features.quit_dialog_visible("Quit glass-core?"))


if __name__ == "__main__":
    unittest.main()
