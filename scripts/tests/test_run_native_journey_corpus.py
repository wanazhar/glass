import importlib.util
from pathlib import Path
import unittest


SCRIPT = Path(__file__).parents[1] / "run-native-journey-corpus.py"
SPEC = importlib.util.spec_from_file_location("native_journey_corpus", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
corpus = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(corpus)


class JourneyFailureClassificationTests(unittest.TestCase):
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

    def test_missing_observed_target_is_a_product_failure(self):
        message = "unsupported operation action: native action target was not found"
        self.assertEqual(corpus.classify(message), "product_bug")
        self.assertEqual(corpus.error_kind(message), "native_target_not_found")

    def test_ambiguous_observed_target_is_a_product_failure(self):
        message = "native action target matched 2 elements; exactly one is required"
        self.assertEqual(corpus.classify(message), "product_bug")
        self.assertEqual(corpus.error_kind(message), "native_target_ambiguous")

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
