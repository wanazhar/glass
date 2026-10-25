import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).parents[1] / "run-native-journey-corpus.py"
SPEC = importlib.util.spec_from_file_location("native_journey_corpus", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
corpus = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(corpus)


class JourneyFailureClassificationTests(unittest.TestCase):
    def test_wikipedia_journey_uses_observed_link_and_asserts_both_articles(self):
        import json

        manifest = corpus.DEFAULT_CORPUS
        journeys = json.loads(manifest.read_text(encoding="utf-8"))["journeys"]
        journey = next(
            item
            for item in journeys
            if item["id"] == "public-wikipedia-article-and-observed-subpage"
        )
        steps = journey["steps"]
        self.assertEqual(steps[0]["url"], "https://en.wikipedia.org/wiki/Computer")
        conditions = [
            step.get("condition") for step in steps if step["action"] == "assert"
        ]
        self.assertIn("title-contains=Computer", conditions)
        click = next(step for step in steps if step["id"] == "choose-observed-subpage")
        self.assertEqual(
            corpus.observed_click_step(click),
            {"action": "clickObserved", "role": "link", "nameContains": "computer science"},
        )
        self.assertIn("title-contains=Computer science", conditions)
        self.assertIn(
            "url-prefix=https://en.wikipedia.org/wiki/Computer_science", conditions
        )

    def test_observed_click_compiles_to_fresh_semantic_selector(self):
        self.assertEqual(
            corpus.observed_click_step(
                {"targetSource": "fresh-observation", "role": "link", "nameContains": "library"}
            ),
            {"action": "clickObserved", "role": "link", "nameContains": "library"},
        )

    def test_observed_click_rejects_blind_selector_metadata(self):
        with self.assertRaisesRegex(ValueError, "fresh-observation"):
            corpus.observed_click_step({"targetSource": "fixed-selector", "role": "link", "nameContains": "library"})

    def test_observed_scroll_compiles_to_fresh_explicit_target(self):
        self.assertEqual(
            corpus.observed_scroll_step(
                {"targetSource": "fresh-observation", "tagName": "main", "dy": 400}
            ),
            {"action": "scrollTargetObserved", "dx": 0, "dy": 400, "tagName": "main"},
        )

    def test_observed_scroll_rejects_blind_or_unbounded_selection(self):
        with self.assertRaisesRegex(ValueError, "fresh-observation"):
            corpus.observed_scroll_step({"targetSource": "fallback", "tagName": "main"})
        with self.assertRaisesRegex(ValueError, "nameContains or tagName"):
            corpus.observed_scroll_step({"targetSource": "fresh-observation", "dy": 400})

    def test_viewport_scroll_requires_immediately_preceding_observation(self):
        self.assertEqual(
            corpus.observed_viewport_scroll_step(
                {"targetSource": "observed-viewport", "dy": 400}, "observe"
            ),
            {"action": "scroll", "dx": 0, "dy": 400},
        )
        with self.assertRaisesRegex(ValueError, "immediately before"):
            corpus.observed_viewport_scroll_step(
                {"targetSource": "observed-viewport", "dy": 400}, "navigate"
            )
        with self.assertRaisesRegex(ValueError, "nonzero delta"):
            corpus.observed_viewport_scroll_step(
                {"targetSource": "observed-viewport"}, "observe"
            )

    def test_missing_observed_target_is_a_product_failure(self):
        message = "unsupported operation action: native action target was not found"
        self.assertEqual(corpus.classify(message), "product_bug")
        self.assertEqual(corpus.error_kind(message), "native_target_not_found")

    def test_ambiguous_observed_target_is_a_product_failure(self):
        message = "native action target matched 2 elements; exactly one is required"
        self.assertEqual(corpus.classify(message), "product_bug")
        self.assertEqual(corpus.error_kind(message), "native_target_ambiguous")

    def test_missing_or_ambiguous_observed_scroll_target_is_a_product_failure(self):
        missing = "fresh observation has no matching scroll target"
        ambiguous = "fresh observation has ambiguous scroll targets"
        self.assertEqual(corpus.classify(missing), "product_bug")
        self.assertEqual(corpus.error_kind(missing), "native_scroll_target_not_found")
        self.assertEqual(corpus.classify(ambiguous), "product_bug")
        self.assertEqual(corpus.error_kind(ambiguous), "native_scroll_target_ambiguous")

    def test_unavailable_viewport_scroll_is_typed(self):
        message = "native viewport scroll produced no movement (scrollY=0, documentHeight=720, viewportHeight=720)"
        self.assertEqual(corpus.classify(message), "product_bug")
        self.assertEqual(corpus.error_kind(message), "native_viewport_scroll_unavailable")

    def test_missing_bounded_semantic_target_is_a_product_failure(self):
        message = 'observed target not found: role="link", name="library"'
        self.assertEqual(corpus.classify(message), "product_bug")
        self.assertEqual(corpus.error_kind(message), "native_target_not_found")

    def test_missing_native_visible_semantic_match_is_typed(self):
        message = 'target: no visible target matches role="link", name="library"; roleTargets=30'
        self.assertEqual(corpus.classify(message), "product_bug")
        self.assertEqual(corpus.error_kind(message), "native_target_not_found")

    def test_explicitly_missing_video_decoder_is_unsupported(self):
        message = "video decoding is not supported by the native backend"
        self.assertEqual(corpus.classify(message), "unsupported")

    def test_remote_challenge_is_denied(self):
        message = "remote site returned HTTP 403 access denied"
        self.assertEqual(corpus.classify(message), "denied")


if __name__ == "__main__":
    unittest.main()
