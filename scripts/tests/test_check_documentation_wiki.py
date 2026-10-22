"""Behavior tests for the documentation wiki catalog and alerts."""

from __future__ import annotations

import importlib.util
import pathlib
import subprocess
import tempfile
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location(
    "check_documentation_wiki", ROOT / "scripts/check-documentation-wiki.py"
)
assert SPEC is not None and SPEC.loader is not None
wiki = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(wiki)


class DocumentationWikiTests(unittest.TestCase):
    def test_heading_rename_is_a_section_change(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            (root / "guide.md").write_text("# Guide\n\n## Before\nText\n")
            original_root = wiki.ROOT
            wiki.ROOT = root
            try:
                base = subprocess.check_output(["git", "init", "-q"], cwd=root, text=True)
                subprocess.run(["git", "config", "user.email", "test@example.invalid"], cwd=root, check=True)
                subprocess.run(["git", "config", "user.name", "Fixture"], cwd=root, check=True)
                subprocess.run(["git", "add", "guide.md"], cwd=root, check=True)
                subprocess.run(["git", "commit", "-qm", "base"], cwd=root, check=True)
                sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
                current = {"guide.md": "# Guide\n\n## After\nText\n"}
                report, entries = wiki.diff_report(sha, current)
                self.assertEqual(entries[0]["kind"], "sections-changed")
                self.assertIn("`Before`", report)
                self.assertIn("`After`", report)
            finally:
                wiki.ROOT = original_root

    def test_broken_anchor_is_reported(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            (root / "guide.md").write_text("# Guide\n\n[bad](#missing)\n")
            original_root = wiki.ROOT
            wiki.ROOT = root
            try:
                self.assertTrue(any("broken local anchor" in failure for failure in wiki.check_links(["guide.md"])))
            finally:
                wiki.ROOT = original_root

    def test_new_page_without_specific_topic_is_unrouted(self) -> None:
        self.assertEqual(wiki.unrouted_pages({"new-subsystem-overview.md"}), ["new-subsystem-overview.md"])
        self.assertEqual(wiki.unrouted_pages({"docs/task-queue-guide.md"}), [])

    def test_new_topically_classified_page_still_needs_index_link(self) -> None:
        original_root = wiki.ROOT
        with tempfile.TemporaryDirectory() as temporary:
            wiki.ROOT = pathlib.Path(temporary)
            try:
                self.assertEqual(
                    wiki.unlinked_current_pages({"products/new-guide.md"}, "# Index\n"),
                    ["products/new-guide.md"],
                )
                self.assertEqual(
                    wiki.unlinked_current_pages(
                        {"products/new-guide.md"}, "[Guide](../products/new-guide.md)\n"
                    ),
                    [],
                )
            finally:
                wiki.ROOT = original_root

    def test_body_only_edit_is_distinguished(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            (root / "guide.md").write_text("# Guide\n\nFirst body.\n")
            original_root = wiki.ROOT
            wiki.ROOT = root
            try:
                subprocess.run(["git", "init", "-q"], cwd=root, check=True)
                subprocess.run(["git", "config", "user.email", "test@example.invalid"], cwd=root, check=True)
                subprocess.run(["git", "config", "user.name", "Fixture"], cwd=root, check=True)
                subprocess.run(["git", "add", "guide.md"], cwd=root, check=True)
                subprocess.run(["git", "commit", "-qm", "base"], cwd=root, check=True)
                sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
                report, entries = wiki.diff_report(sha, {"guide.md": "# Guide\n\nChanged body.\n"})
                self.assertEqual(entries[0]["kind"], "body-only")
                self.assertIn("Body-only edit", report)
                self.assertIn("does not assess semantic freshness", report)
            finally:
                wiki.ROOT = original_root

    def test_renamed_page_and_section_changes_are_reported(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = pathlib.Path(temporary)
            common = "A long paragraph shared by both versions of this guide to make the move recognizable.\n"
            (root / "old.md").write_text("# Guide\n\n## Before\n\n" + common)
            original_root = wiki.ROOT
            wiki.ROOT = root
            try:
                subprocess.run(["git", "init", "-q"], cwd=root, check=True)
                subprocess.run(["git", "config", "user.email", "test@example.invalid"], cwd=root, check=True)
                subprocess.run(["git", "config", "user.name", "Fixture"], cwd=root, check=True)
                subprocess.run(["git", "add", "old.md"], cwd=root, check=True)
                subprocess.run(["git", "commit", "-qm", "base"], cwd=root, check=True)
                sha = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip()
                (root / "old.md").unlink()
                current = {"renamed.md": "# Guide\n\n## After\n\n" + common}
                _, entries = wiki.diff_report(sha, current)
                self.assertEqual(entries[0]["kind"], "renamed")
                self.assertEqual(entries[0]["from"], "old.md")
                self.assertEqual(entries[0]["to"], "renamed.md")
                self.assertEqual(entries[0]["sectionsAdded"], ["After"])
                self.assertEqual(entries[0]["sectionsRemoved"], ["Before"])
            finally:
                wiki.ROOT = original_root

    def test_historical_classification_is_path_based(self) -> None:
        self.assertEqual(wiki.classify("docs/plan/tasks/old-slice.md"), "historical")


if __name__ == "__main__":
    unittest.main()
