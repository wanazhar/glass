import importlib.util
import os
import sys
import unittest
from pathlib import Path
from unittest.mock import patch

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
# Palette row plus a search label. Exact "Work" header is absent.
PALETTE_WORK_AND_FIND = (
    "Commands            \n"
    "❯ /work             \n"
    "› Work              \n"
    "Find:               \n"
    "Enter · Esc close   "
)
PALETTE_WORK_AND_SEARCH = (
    "› Work              \n"
    "Search:             \n"
    "guidance            "
)

# Actual 40x24 Commands frame from boxed-title-smoke.log (exit 1). Trailing spaces kept.
CAPTURED_COMMANDS_40X24 = """\
                                        
                                        
                                        
                                        
                                        
                                        
                                        
  ╭──────────────────────────────────╮  
  │ Commands · search or * browse al │  
  │ ❯ /workspace                     │  
  │                                  │  
  │ › Work                           │  
  │   checkpoint                     │  
  │   profiles                       │  
  │ Changes, runs, teammates, tasks  │  
  │ Enter · Esc close                │  
  ╰──────────────────────────────────╯  
                                        
                                        
                                        
                                        
                                        
                                        
                                        
"""

# Actual 20x8 workspace frame from oracle-smoke-final.log (exit 1).
CAPTURED_WORK_20X8 = '                                        \n ╭────────────────────────────────────╮ \n │ Work                               │ \n │                                    │ \n │ Find:                              │ \n │ › Task 1 · done · parser           │ \n │   Task 2 · claimed · integration   │ \n │   Task 3 · pending · overlap       │ \n │   /desktop · Choose quick XFCE or  │ \n │   /todo · Inspect and manage the s │ \n │   /queue-run · Run saved follow-up │ \n │   /turns · Review observed changes │ \n │   /constraints · Opt-in turn-bound │ \n │   /advisor · Review cadence, notes │ \n │   /pr · Pull request, CI and revie │ \n │   /outputs · Native structured res │ \n │   /code-review · Review the diff f │ \n │   /retry · Recover a failed respon │ \n │   /agents · Persistent teammates,  │ \n │   /thinking · View live provider r │ \n │   /review · Review task summary, c │ \n │ ↵ · Esc · → details                │ \n ╰────────────────────────────────────╯ \n                                        '


def retired_substring_work_overlay(text):
    """Old oracle. Substring Work plus Find/Search. Not used by the smoke."""
    return "Work" in text and ("Find:" in text or "Search:" in text)


def plain_exact_header(text, title):
    """Prior smoke matcher. Rejects the captured boxed Work row."""
    return any(line.strip() == title for line in text.splitlines())


CAPTURED_CONVERSATIONS_80X30 = '                                                                                \n                                                                                \n                                                                                \n                                                                                \n ╭────────────────────────────────────────────────────────────────────────────╮ \n │ Conversations                                                              │ \n │                                                                            │ \n │ All projects · page 1/1 · search this page; search all below               │ \n │ Search:                                                                    │ \n │                                                                            │ \n │ › Untitled conversation · mwa54rf9-37c9ae-0000 · Created in GC · 0 unread  │ \n │   Untitled conversation · mwa54qth-37c9aa-0000 · Created in GC · 0 unread  │ \n │   Untitled conversation · mwa54q7w-37c9a7-0000 · Created in GC · 0 unread  │ \n │   Fix addition in sample.py and verify it with its · mwa54pt4-37c981-0000  │ \n │   hello · mwa54psu-37c97e-0000 · Created in GC · 0 unread events           │ \n │   Search all indexed conversations…                                        │ \n │   Current project only                                                     │ \n │   Next page                                                                │ \n │   Previous page                                                            │ \n │ Preview · F1 to read in full                                               │ \n │ │ Untitled conversation · mwa54rf9-37c9ae-0000 · Created in GC · 0 unread  │ \n │ │ events 2026-11-10 03:38 /tmp/gc-workflow-next-eu21ea8m/workspace with    │ \n │ │ spaces                                                                   │ \n │ 9 choices · F1/→ full text                                                 │ \n │ ↑↓ select · Enter choose · Esc back · F1 details                           │ \n ╰────────────────────────────────────────────────────────────────────────────╯ \n                                                                                \n                                                                                \n                                                                                \n                                                                                '

RESUME_CONVERSATION_LABEL = (
    "Commands\n"
    "\u203a Resume conversation\n"
    "  Work\n"
)

class WorkflowPtyReadyTests(unittest.TestCase):

    def test_accepts_captured_80x30_conversations_picker_not_resume_label(self):
        frame = CAPTURED_CONVERSATIONS_80X30
        self.assertEqual(len(frame.splitlines()), 30)
        self.assertTrue(all(len(line) == 80 for line in frame.splitlines()))
        self.assertIn("All projects \u00b7", frame)
        self.assertNotIn("Resume conversation", frame)
        self.assertFalse(plain_exact_header(frame, "Conversations"))
        self.assertTrue(workflow.conversations_picker_ready(frame))
        self.assertIn("Resume conversation", RESUME_CONVERSATION_LABEL)
        self.assertFalse(workflow.conversations_picker_ready(RESUME_CONVERSATION_LABEL))
        mixed = "\u2502 Conversations \u2502\nAll projects \u00b7 page 1/1\nResume conversation\n"
        self.assertFalse(workflow.conversations_picker_ready(mixed))
        self.assertTrue(workflow.conversations_picker_ready("\u2502 Conversations \u2502\nAll projects \u00b7 page 1/1\nSearch:\n"))

    def test_accepts_compact_work_chooser_from_ci(self):
        self.assertTrue(workflow.workspace_overlay_ready(CI_20X8))
        self.assertNotIn("Workspace", CI_20X8)

    def test_rejects_palette_work_row_before_submit(self):
        self.assertFalse(workflow.workspace_overlay_ready(PALETTE_WORK_ROW))

    def test_rejects_work_substring_without_header(self):
        embedded = "Find:\n› Task · Work item\n"
        self.assertFalse(workflow.workspace_overlay_ready(embedded))
        self.assertNotIn("Commands", embedded.splitlines()[0])

    def test_accepts_wide_work_chooser(self):
        self.assertTrue(workflow.workspace_overlay_ready(WIDE_WORK))

    def test_quit_dialog_matches_rendered_confirmation(self):
        self.assertTrue(features.quit_dialog_visible("╭ EXIT · confirm ──╮\n│ QUIT?            │"))
        self.assertTrue(features.quit_dialog_visible("Quit?\n↵ exit"))
        self.assertFalse(features.quit_dialog_visible("Quit glass-core?"))

    def test_rejects_palette_work_and_find_that_old_substring_accepts(self):
        for fixture in (PALETTE_WORK_AND_FIND, PALETTE_WORK_AND_SEARCH):
            self.assertIn("› Work", fixture)
            self.assertFalse(workflow.workspace_overlay_ready(fixture))
            self.assertTrue(retired_substring_work_overlay(fixture))

    def test_accepts_captured_20x8_boxed_work_frame(self):
        self.assertIn("Find:", CAPTURED_WORK_20X8)
        self.assertFalse(plain_exact_header(CAPTURED_WORK_20X8, "Work"))
        self.assertTrue(retired_substring_work_overlay(CAPTURED_WORK_20X8))
        self.assertTrue(workflow.workspace_overlay_ready(CAPTURED_WORK_20X8))
        self.assertTrue(workflow.workspace_overlay_ready("│ Work │\nSearch:"))
        self.assertFalse(workflow.workspace_overlay_ready("│ Work │\n"))

    def test_rejects_boxed_non_exact_work_titles(self):
        for fixture in (
            "│ › Work │\nFind:",
            "│ › Work │\nSearch:",
            "│ My Work │\nFind:",
            "│ Workspace │\nFind:",
            "│ Work extra │\nFind:",
        ):
            self.assertFalse(workflow.workspace_overlay_ready(fixture), fixture)
            self.assertTrue("Work" in fixture)

    def test_dismiss_second_esc_after_boxed_commands(self):
        boxed = _Screen(["│ Commands │", "❯ /work"], y=1)
        self.assertTrue(any(features.painted_header_is(line, "Commands") for line in boxed.text().splitlines()))
        self.assertFalse(plain_exact_header(boxed.text(), "Commands"))
        tty = _dismiss_tty(["│ Work │", "Find:"], y=0)
        writes = []
        frames = [boxed, _Screen(["    ", "    ", "    ", "❯ ready"], y=3)]

        def read():
            if frames:
                tty.screen = frames.pop(0)

        tty.read = read

        def select_fn(channels, *_args):
            return ([channels[0]], [], []) if frames else ([], [], [])

        with _instant_io(writes, select_fn=select_fn):
            features.Terminal.dismiss(tty)
        self.assertEqual(writes, [b"\x1b", b"\x1b"])
        self.assertEqual(frames, [])

    def test_dismiss_second_esc_after_captured_40x24_clipped_commands(self):
        self.assertEqual(len(CAPTURED_COMMANDS_40X24.splitlines()), 24)
        self.assertEqual(len(CAPTURED_COMMANDS_40X24.splitlines()[8]), 40)
        self.assertIn("❯ /workspace", CAPTURED_COMMANDS_40X24)
        self.assertIn("› Work", CAPTURED_COMMANDS_40X24)
        self.assertIn("Enter · Esc close", CAPTURED_COMMANDS_40X24)
        self.assertFalse(any(features.painted_header_is(line, "Commands") for line in CAPTURED_COMMANDS_40X24.splitlines()))
        self.assertTrue(any(features.painted_commands_header(line) for line in CAPTURED_COMMANDS_40X24.splitlines()))
        full = "│ " + features.COMMANDS_ANNOTATED_TITLE + " │"
        self.assertTrue(features.painted_commands_header(full))
        tty = _dismiss_tty(["│ Work │", "Find:"], y=0)
        writes = []
        frames = [
            _Paint(CAPTURED_COMMANDS_40X24),
            _Screen(["    ", "    ", "    ", "❯ ready"], y=3),
        ]

        def read():
            if frames:
                tty.screen = frames.pop(0)

        tty.read = read

        def select_fn(channels, *_args):
            return ([channels[0]], [], []) if frames else ([], [], [])

        with _instant_io(writes, select_fn=select_fn):
            features.Terminal.dismiss(tty)
        self.assertEqual(writes, [b"\x1b", b"\x1b"])
        self.assertEqual(frames, [])

    def test_dismiss_rejects_clipped_commands_lookalikes_without_a_second_esc(self):
        for line in (
            "│ Commands-other text │",
            "Commands-other text",
            "│ My Commands │",
            "My Commands",
            "│ › Commands │",
            "│ Commands · sea │",
            "│ Command │",
        ):
            self.assertFalse(features.painted_commands_header(line), line)
            tty = _dismiss_tty(["│ Work │", "Find:"], y=0)
            writes = []

            def read(screen=_Paint(line, y=0, height=6)):
                tty.screen = screen

            tty.read = read

            def select_fn(channels, *_args):
                return ([channels[0]], [], [])

            with _instant_io(writes, select_fn=select_fn):
                with self.assertRaises(AssertionError):
                    features.Terminal.dismiss(tty)
            self.assertEqual(writes, [b"\x1b"], line)

    def test_dismiss_rejects_boxed_chevron_commands_without_a_second_esc(self):
        bad = _Screen(["│ › Commands │"], y=0)
        self.assertFalse(any(features.painted_header_is(line, "Commands") for line in bad.text().splitlines()))
        tty = _dismiss_tty(["│ Work │", "Find:"], y=0)
        writes = []

        def read():
            tty.screen = bad

        tty.read = read

        def select_fn(channels, *_args):
            return ([channels[0]], [], [])

        with _instant_io(writes, select_fn=select_fn):
            with self.assertRaises(AssertionError):
                features.Terminal.dismiss(tty)
        self.assertEqual(writes, [b"\x1b"])

    def test_dismiss_caps_at_two_esc_on_boxed_commands(self):
        tty = _dismiss_tty(["│ Work │", "Find:"], y=0)
        writes = []
        palette = _Screen(["│ Commands │", "❯ /workspace"], y=1)

        def read():
            tty.screen = palette

        tty.read = read

        def select_fn(channels, *_args):
            return ([channels[0]], [], [])

        with _instant_io(writes, select_fn=select_fn):
            with self.assertRaises(AssertionError):
                features.Terminal.dismiss(tty)
        self.assertEqual(writes, [b"\x1b", b"\x1b"])

    def test_dismiss_sends_no_esc_when_composer_already_visible(self):
        tty = _dismiss_tty(["    ", "    ", "    ", "❯ ready"], y=3)
        writes = []
        with _instant_io(writes):
            features.Terminal.dismiss(tty)
        self.assertEqual(writes, [])

    def test_dismiss_stops_after_one_esc_when_composer_appears(self):
        tty = _dismiss_tty(["overlay"], y=0)
        writes = []

        def write(_fd, data):
            writes.append(data)
            tty.screen = _Screen(["    ", "    ", "    ", "❯ ready"], y=3)

        with _instant_io(writes, write=write):
            features.Terminal.dismiss(tty)
        self.assertEqual(writes, [b"\x1b"])

    def test_dismiss_second_esc_only_after_post_key_exact_commands(self):
        tty = _dismiss_tty(["Work", "Find:"], y=0)
        writes = []
        frames = [
            _Screen(["Commands", "❯ /work"], y=1),
            _Screen(["    ", "    ", "    ", "❯ ready"], y=3),
        ]

        def read():
            if frames:
                tty.screen = frames.pop(0)

        tty.read = read

        def select_fn(channels, *_args):
            return ([channels[0]], [], []) if frames else ([], [], [])

        with _instant_io(writes, select_fn=select_fn):
            features.Terminal.dismiss(tty)
        self.assertEqual(writes, [b"\x1b", b"\x1b"])
        self.assertEqual(frames, [])

    def test_dismiss_rejects_non_exact_commands_without_a_second_esc(self):
        for lines in (["› Commands"], ["Commands:"], ["Find:"], ["My Commands"], ["Commands"]):
            tty = _dismiss_tty(lines, y=0)
            writes = []
            with _instant_io(writes):
                with self.assertRaises(AssertionError):
                    features.Terminal.dismiss(tty)
            self.assertEqual(writes, [b"\x1b"], lines)

    def test_dismiss_caps_at_two_esc_when_commands_never_reaches_composer(self):
        tty = _dismiss_tty(["Work", "Find:"], y=0)
        writes = []
        palette = _Screen(["Commands", "❯ /workspace"], y=1)

        def read():
            tty.screen = palette

        tty.read = read

        def select_fn(channels, *_args):
            return ([channels[0]], [], [])

        with _instant_io(writes, select_fn=select_fn):
            with self.assertRaises(AssertionError):
                features.Terminal.dismiss(tty)
        self.assertEqual(writes, [b"\x1b", b"\x1b"])

    def test_close_confirms_only_after_quit_dialog_then_exit_zero(self):
        proc = _Proc()
        tty = _close_tty(proc, "composer")
        writes = []

        def write(_fd, data):
            writes.append(data)
            if data == b"\x03":
                tty.screen = _Text("│ QUIT?            │")
            elif data == b"\r":
                self.assertTrue(features.quit_dialog_visible(tty.screen.text()))
                proc.returncode = 0

        with _instant_io(writes, write=write):
            features.Terminal.close(tty)
        self.assertEqual(proc.returncode, 0)
        self.assertIn(b"\r", writes)
        self.assertLess(writes.index(b"\x03"), writes.index(b"\r"))
        self.assertNotIn(b"\x04", writes)
        self.assertTrue(proc.waited)

    def test_close_rejects_exit_zero_without_observing_dialog(self):
        proc = _Proc()
        tty = _close_tty(proc, "still working")
        writes = []

        def write(_fd, data):
            writes.append(data)
            if data == b"\x03":
                proc.returncode = 0

        with _instant_io(writes, write=write):
            with self.assertRaisesRegex(AssertionError, "quit dialog was not observed"):
                features.Terminal.close(tty)
        self.assertEqual(proc.returncode, 0)
        self.assertNotIn(b"\r", writes)
        self.assertTrue(proc.waited)
        self.assertTrue(tty.closed)

    def test_close_rejects_already_exited_zero_without_dialog(self):
        proc = _Proc()
        proc.returncode = 0
        tty = _close_tty(proc, "gone")
        writes = []
        with _instant_io(writes):
            with self.assertRaisesRegex(AssertionError, "quit dialog was not observed"):
                features.Terminal.close(tty)
        self.assertEqual(writes, [])
        self.assertTrue(proc.waited)
        self.assertTrue(tty.closed)

    def test_close_reaps_without_masking_an_earlier_failure(self):
        proc = _Proc()
        tty = _close_tty(proc, "composer")
        tty.read = _boom
        writes = []

        def select_ready(channels, *_args):
            return [channels[0]]

        with _instant_io(writes, select_fn=select_ready):
            with self.assertRaisesRegex(RuntimeError, "original failure"):
                features.Terminal.close(tty)
        self.assertTrue(proc.waited)
        self.assertTrue(tty.closed)

    def test_close_during_unwind_reaps_without_replacing_caller_error(self):
        proc = _Proc()
        tty = _close_tty(proc, "\u2502 Conversations \u2502")
        writes = []
        observed = None
        try:
            try:
                raise AssertionError("picker title missing")
            finally:
                with _instant_io(writes):
                    features.Terminal.close(tty)
        except AssertionError as error:
            observed = str(error)
        self.assertEqual(observed, "picker title missing")
        self.assertTrue(proc.waited)
        self.assertTrue(tty.closed)
        self.assertTrue(proc.terminated)
        self.assertNotIn(b"\r", writes)


class _Screen:
    def __init__(self, lines, y):
        self.height = 6
        self.y = y
        width = 20
        self.cells = [list(line.ljust(width)[:width]) for line in lines]
        while len(self.cells) < self.height:
            self.cells.append([" "] * width)

    def text(self):
        return "\n".join("".join(row) for row in self.cells)


class _Text:
    def __init__(self, text):
        self._text = text

    def text(self):
        return self._text


class _Paint:
    def __init__(self, text, y=0, height=24):
        self.y = y
        self.height = height
        self.cells = [[" "] * 4 for _ in range(height)]
        self._text = text

    def text(self):
        return self._text


class _Proc:
    def __init__(self):
        self.returncode = None
        self.waited = False
        self.terminated = False

    def poll(self):
        return self.returncode

    def wait(self):
        self.waited = True
        if self.returncode is None:
            self.returncode = -15
        return self.returncode

    def terminate(self):
        self.terminated = True
        if self.returncode is None:
            self.returncode = -15

    def kill(self):
        self.terminated = True
        if self.returncode is None:
            self.returncode = -9


def _dismiss_tty(lines, y):
    tty = features.Terminal.__new__(features.Terminal)
    tty.master = 3
    tty.received = "stale"
    tty.screen = _Screen(lines, y)
    tty.read = lambda: None
    return tty


def _boom():
    raise RuntimeError("original failure")


class _Clock:
    def __init__(self):
        self.t = 1000.0

    def __call__(self):
        self.t += 0.01
        return self.t


def _instant_io(writes, write=None, select_fn=None):
    def default_write(_fd, data):
        writes.append(data)

    def default_select(_channels, *_args):
        return ([], [], [])

    return _patches(write or default_write, select_fn or default_select)


class _patches:
    def __init__(self, write, select_fn):
        self.write = write
        self.select_fn = select_fn
        self.stack = None

    def __enter__(self):
        from contextlib import ExitStack
        self.stack = ExitStack()
        self.stack.enter_context(patch.object(os, "write", self.write))
        self.stack.enter_context(patch.object(features.select, "select", self.select_fn))
        self.stack.enter_context(patch.object(features.time, "monotonic", _Clock()))
        real_close = os.close

        def tracking_close(fd):
            if fd == 7:
                _close_holder[0].closed = True
                return
            real_close(fd)

        self.stack.enter_context(patch.object(os, "close", tracking_close))
        return self

    def __exit__(self, *exc):
        return self.stack.__exit__(*exc)


_close_holder = [None]


def _close_tty(proc, text):
    tty = features.Terminal.__new__(features.Terminal)
    tty.master = 7
    tty.process = proc
    tty.screen = _Text(text)
    tty.read = lambda: None
    tty.closed = False
    _close_holder[0] = tty
    return tty


if __name__ == "__main__":
    unittest.main()
