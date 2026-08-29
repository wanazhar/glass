#!/usr/bin/env python3
"""Contract tests for the dynamic release-documentation audit."""

import importlib.util
import pathlib
import unittest


ROOT = pathlib.Path(__file__).resolve().parents[2]
MODULE_PATH = ROOT / "scripts/check-release-documentation.py"
SPEC = importlib.util.spec_from_file_location("check_release_documentation", MODULE_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"cannot load {MODULE_PATH}")
CHECKER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKER)


class ReleaseDocumentationContractTests(unittest.TestCase):
    def test_required_markers_follow_the_current_and_previous_versions(self) -> None:
        markers = CHECKER.required_markers("9.9.9", "9.9.8")

        self.assertIn(
            "| `glass-browser 9.9.9`, `glass-dev 9.9.9` |",
            markers["README.md"],
        )
        self.assertIn(
            "# Migrate from 9.9.8 to 9.9.9",
            markers["docs/migration/9.9.9.md"],
        )
        self.assertIn("## [9.9.9]", markers["CHANGELOG.md"])

    def test_current_docs_rs_claim_using_previous_version_is_blocking(self) -> None:
        text = (
            "Published docs.rs pages match the crate version they were built from "
            "(`0.3.13` at last publication)."
        )

        hits = CHECKER.find_previous_version_hits(
            "docs/features.md", text, "0.3.13"
        )

        self.assertEqual(len(hits), 1)
        self.assertTrue(hits[0]["current_claim"])
        self.assertIn("docs.rs", hits[0]["reasons"][0])

    def test_current_release_claim_using_previous_version_is_blocking(self) -> None:
        text = "The latest published release is `0.3.13`."

        hits = CHECKER.find_previous_version_hits(
            "docs/rust-sdk.md", text, "0.3.13"
        )

        self.assertEqual(len(hits), 1)
        self.assertTrue(hits[0]["current_claim"])

    def test_historical_and_record_context_is_reported_but_not_current_claim(self) -> None:
        historical = CHECKER.find_previous_version_hits(
            "docs/releases/0.3.13.md",
            "Published docs.rs pages for 0.3.13 are historical.",
            "0.3.13",
        )
        record = CHECKER.find_previous_version_hits(
            "docs/release-checklist.md",
            "The previous published release was 0.3.13.",
            "0.3.13",
        )

        self.assertEqual(historical[0]["classification"], "historical")
        self.assertFalse(historical[0]["current_claim"])
        self.assertEqual(record[0]["classification"], "record")
        self.assertFalse(record[0]["current_claim"])

    def test_contextual_previous_version_reference_is_not_misclassified(self) -> None:
        hits = CHECKER.find_previous_version_hits(
            "README.md", "`0.3.13` | Previous published stable release", "0.3.13"
        )

        self.assertEqual(len(hits), 1)
        self.assertFalse(hits[0]["current_claim"])
        self.assertEqual(hits[0]["reasons"], [])

    def test_semantic_audit_records_navigation_and_version_sensitive_lines(self) -> None:
        hits = CHECKER.find_semantic_audit_hits(
            "README.md", "The current release uses Ctrl-L; previous is 0.3.13.", "0.3.13"
        )

        self.assertEqual(len(hits), 1)
        self.assertEqual(hits[0]["classification"], "current")

    def test_discovery_includes_markdown_outside_the_public_doc_directories(self) -> None:
        documents = {
            path.relative_to(ROOT).as_posix()
            for path in CHECKER.discover_markdown_documents(ROOT)
        }

        self.assertIn("CONTRIBUTING.md", documents)
        self.assertIn("benchmarks/README.md", documents)
        self.assertIn("fuzz/README.md", documents)

    def test_new_unclassified_markdown_defaults_to_current_claim_checks(self) -> None:
        hits = CHECKER.find_previous_version_hits(
            "notes/new-guide.md", "The latest published release is 0.3.13.", "0.3.13"
        )

        self.assertTrue(hits[0]["current_claim"])
        self.assertEqual(hits[0]["classification"], "current")

    def test_historical_results_and_generated_assets_are_classified(self) -> None:
        self.assertEqual(
            CHECKER.document_classification("benchmarks/results/run.md"),
            "historical",
        )
        self.assertEqual(
            CHECKER.document_classification("crates/glass-dev/assets/prompt.md"),
            "generated",
        )


if __name__ == "__main__":
    unittest.main()
