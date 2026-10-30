import importlib.util
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest


SCRIPT = Path(__file__).parents[1] / "run-native-journey-corpus.py"
SPEC = importlib.util.spec_from_file_location("native_journey_corpus", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
corpus = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(corpus)


class JourneyFailureClassificationTests(unittest.TestCase):
    def test_public_report_url_drops_credentials_query_and_fragment(self):
        safe = corpus.safe_report_url(
            "https://user:pass@developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/img?token=URL_SENTINEL#FRAGMENT_SENTINEL",
            "https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/img",
        )
        self.assertEqual(
            safe,
            "https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/img",
        )
        self.assertNotIn("SENTINEL", safe)

    def test_public_report_url_hides_off_host_redirect(self):
        self.assertEqual(
            corpus.safe_report_url(
                "https://private.example/path?token=URL_SENTINEL",
                "https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/img",
            ),
            "external_redirect",
        )
        self.assertEqual(
            corpus.safe_report_url(
                "https://developer.mozilla.org/en-US/docs/users/USER_SENTINEL",
                "https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/img",
            ),
            "unexpected_path",
        )

    def test_title_is_mapped_to_static_allowlisted_label(self):
        title = corpus.safe_title(
            "HTMLImageElement: The Image Embed element - HTML: HyperText Markup Language | MDN"
        )
        self.assertEqual(title, "MDN HTML image reference")
        unknown = corpus.safe_title("MDN img?token=TITLE_SENTINEL#frag")
        self.assertIsNone(unknown)
        self.assertNotIn("TITLE_SENTINEL", str(unknown))

    def test_image_diagnostics_are_capped_typed_and_source_free(self):
        diagnostics = corpus.safe_image_diagnostics({
            "supported": True,
            "readyState": "complete",
            "title": "image?token=TITLE_SENTINEL",
            "imageCount": 10001,
            "imageCountOverflow": True,
            "loadedCount": 0,
            "sourcePresentCount": 1,
            "incompleteWithSourceCount": 0,
            "completeWithoutDimensionsCount": 1,
            "visibleCount": 0,
            "lazyHintCount": 0,
            "decodeApiSupported": True,
            "images": [{
                "complete": True,
                "width": 99999,
                "height": 0,
                "sourcePresent": True,
                "visible": False,
                "src": "https://private.example/SECRET_SENTINEL",
                "alt": "ALT_SENTINEL",
            }],
        })
        serialized = json.dumps(diagnostics)
        self.assertEqual(diagnostics["category"], "image_set_overflow_inconclusive")
        self.assertEqual(diagnostics["imageCount"], 10000)
        self.assertEqual(diagnostics["images"][0]["dimensions"], [16384, 0])
        for sentinel in ("TITLE_SENTINEL", "SECRET_SENTINEL", "ALT_SENTINEL"):
            self.assertNotIn(sentinel, serialized)

    def test_image_diagnostics_distinguish_unsupported_lazy_and_decoded_states(self):
        unsupported = corpus.safe_image_diagnostics({
            "supported": False,
            "failureClass": "image_dom_unsupported",
        })
        lazy = corpus.safe_image_diagnostics({
            "supported": True,
            "imageCount": 1,
            "sourcePresentCount": 1,
            "incompleteWithSourceCount": 1,
            "lazyHintCount": 1,
        })
        decoded = corpus.safe_image_diagnostics({
            "supported": True,
            "imageCount": 1,
            "loadedCount": 1,
            "sourcePresentCount": 1,
            "images": [{"complete": True, "width": 2, "height": 2, "sourcePresent": True}],
        })
        self.assertEqual(unsupported["category"], "javascript_unsupported")
        self.assertEqual(lazy["category"], "lazy_or_pending_image_load")
        self.assertEqual(decoded["category"], "decoded_image_present")
        unresolved = corpus.safe_image_diagnostics({
            "supported": True,
            "imageCount": 1,
            "sourcePresentCount": 1,
            "completeWithoutDimensionsCount": 1,
        })
        self.assertEqual(unresolved["category"], "fetch_or_decode_failure_unresolved")

    def test_safe_worker_diagnostics_keep_only_static_allowlisted_classes(self):
        diagnostics = corpus.safe_worker_diagnostics(
            "native content worker exited with diagnostic class: worker\n"
            "native content worker operation class: content_process_script\n"
            "native content worker reason class: content_process_pipe_closed\n"
            "native content worker private operation: https://private.example/secret\n"
            "native content worker reason class: private-page-text"
        )

        self.assertEqual(
            diagnostics,
            [{
                "class": "worker",
                "operation": "content_process_script",
                "reason": "content_process_pipe_closed",
            }],
        )

    def test_javascript_failure_diagnostics_keep_stack_class_and_hide_page_text(self):
        diagnostics = corpus.safe_worker_diagnostics(
            "native content worker exited with diagnostic class: worker\n"
            "native content worker operation class: content_process_script\n"
            "native content worker reason class: javascript_stack_overflow\n"
            "native content worker private operation: user-supplied-script\n"
            "private page url and contents"
        )

        self.assertEqual(
            diagnostics,
            [{
                "class": "worker",
                "operation": "content_process_script",
                "reason": "javascript_stack_overflow",
            }],
        )

    def test_outer_deadline_is_distinct_from_per_step_timeout(self):
        self.assertEqual(corpus.journey_outer_timeout_ms({"timeoutMs": 120000}), 130000)
        self.assertEqual(
            corpus.journey_outer_timeout_ms(
                {"timeoutMs": 120000, "runnerTimeoutMs": 360000}
            ),
            360000,
        )
        for invalid in [True, "360000", 999, 600001]:
            with self.assertRaises(ValueError):
                corpus.journey_outer_timeout_ms({"runnerTimeoutMs": invalid})

    def test_wikipedia_outer_deadline_allows_the_multi_step_read_only_journey(self):
        journeys = json.loads(corpus.DEFAULT_CORPUS.read_text(encoding="utf-8"))["journeys"]
        journey = next(
            item
            for item in journeys
            if item["id"] == "public-wikipedia-article-and-observed-subpage"
        )
        self.assertEqual(journey["timeoutMs"], 120000)
        self.assertEqual(corpus.journey_outer_timeout_ms(journey), 360000)

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
            {"action": "clickObserved", "role": "link", "nameEquals": "Computer science"},
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

    def test_observed_click_supports_exact_name_and_rejects_mixed_or_missing_name(self):
        self.assertEqual(
            corpus.observed_click_step(
                {"targetSource": "fresh-observation", "role": "link", "nameEquals": "Computer science"}
            ),
            {"action": "clickObserved", "role": "link", "nameEquals": "Computer science"},
        )
        for step in (
            {"targetSource": "fresh-observation", "role": "link"},
            {"targetSource": "fresh-observation", "role": "link", "nameContains": "computer", "nameEquals": "Computer"},
        ):
            with self.assertRaisesRegex(ValueError, "exactly one"):
                corpus.observed_click_step(step)

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

    def test_public_mdn_video_journey_checks_real_playback_not_a_poster(self):
        journeys = json.loads(corpus.DEFAULT_CORPUS.read_text(encoding="utf-8"))["journeys"]
        journey = next(
            item for item in journeys if item["id"] == "public-mdn-cc0-webm-playback"
        )
        self.assertEqual(journey["backend"], "native")
        self.assertEqual(journey["scope"], "public-read-only")
        self.assertEqual(journey["asset"]["license"], "CC0-1.0")
        steps = corpus.compile_public_steps(journey)
        self.assertEqual(
            [step["action"] for step in steps],
            ["navigate", "observe", "wait", "evaluate", "wait", "evaluate", "wait", "screenshot"],
        )
        self.assertIn("currentSrc.includes('flower.webm')", steps[2]["condition"])
        self.assertIn("video.readyState >= 2", steps[2]["condition"])
        self.assertIn("video.play()", steps[3]["expression"])
        self.assertIn("video.currentTime >= 0.25", steps[4]["condition"])
        self.assertIn("video.pause()", steps[5]["expression"])
        self.assertIn("video.paused", steps[6]["condition"])

    def test_public_mdn_image_journey_is_deep_native_and_diagnostic_bounded(self):
        journeys = json.loads(corpus.DEFAULT_CORPUS.read_text(encoding="utf-8"))["journeys"]
        journey = next(item for item in journeys if item["id"] == "public-mdn-html-image-element")
        self.assertEqual(journey["scope"], "public-read-only")
        self.assertEqual(journey["backend"], "native")
        self.assertEqual(
            journey["steps"][0]["url"],
            "https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/img",
        )
        steps = corpus.compile_public_steps(journey)
        self.assertEqual([step["action"] for step in steps], ["navigate", "observe", "wait"])
        self.assertEqual(steps[2]["timeout_ms"], 8000)
        self.assertTrue(steps[2]["imageDiagnostics"])
        self.assertIn("naturalWidth > 0", steps[2]["condition"])
        self.assertIn("naturalHeight > 0", steps[2]["condition"])

    def test_public_evaluate_step_is_bounded(self):
        journey = {"timeoutMs": 1000, "steps": [{"action": "evaluate", "expression": " "}]}
        with self.assertRaisesRegex(ValueError, "nonempty expression"):
            corpus.compile_public_steps(journey)
        journey["steps"][0]["expression"] = "x" * 4097
        with self.assertRaisesRegex(ValueError, "4096 characters"):
            corpus.compile_public_steps(journey)

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

    def test_unsatisfied_javascript_predicate_is_not_an_engine_failure(self):
        message = "wait timed out for javascript_predicate"
        self.assertEqual(corpus.classify(message), "inconclusive")
        self.assertEqual(corpus.error_kind(message), "javascript_predicate_timeout")

    def test_remote_challenge_is_denied(self):
        message = "remote site returned HTTP 403 access denied"
        self.assertEqual(corpus.classify(message), "denied")

    def test_remote_rate_limit_is_denied_not_a_product_failure(self):
        message = "network failure during HTTP document request: server returned HTTP 429"
        self.assertEqual(corpus.classify(message), "denied")
        self.assertEqual(corpus.error_kind(message), "remote_rate_limited")

    def test_progress_file_preserves_completed_steps_and_identifies_interrupted_step(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "progress.jsonl"
            path.write_text(
                "\n".join(
                    [
                        json.dumps({"event": "batch_started", "total": 3}),
                        json.dumps({"event": "step_started", "index": 0, "action": "navigate"}),
                        json.dumps({"event": "step_finished", "index": 0, "action": "navigate", "status": "ok", "elapsedMs": 42}),
                        json.dumps({"event": "step_started", "index": 1, "action": "observe"}),
                        '{"event":"step_finished","index":',
                    ]
                ),
                encoding="utf-8",
            )

            results = corpus.progress_step_results(path, 3)

        self.assertEqual([item["status"] for item in results], ["ok", "interrupted"])
        self.assertEqual(results[0]["elapsedMs"], 42)
        self.assertEqual(results[1]["action"], "observe")
        self.assertEqual(results[1]["errorKind"], "runner_deadline")

    def test_public_journey_keeps_progress_when_process_exits_without_json(self):
        with tempfile.TemporaryDirectory(prefix="glass-native-journey-exit-") as directory:
            root = Path(directory)
            fake = root / "fake-browser"
            fake.write_text(
                "#!/bin/sh\n"
                "printf '%s\\n' \\\n"
                "  '{\"event\":\"batch_started\",\"total\":2}' \\\n"
                "  '{\"event\":\"step_started\",\"index\":0,\"action\":\"navigate\"}' \\\n"
                "  '{\"event\":\"step_finished\",\"index\":0,\"action\":\"navigate\",\"status\":\"ok\",\"elapsedMs\":12}' \\\n"
                "  '{\"event\":\"step_started\",\"index\":1,\"action\":\"observe\"}' \\\n"
                "  '{\"event\":\"step_finished\",\"index\":1,\"action\":\"observe\",\"status\":\"error\",\"elapsedMs\":8,\"errorKind\":\"worker\"}' \\\n"
                "  >> \"$GLASS_BROWSER_BATCH_PROGRESS_PATH\"\n"
                "printf '%s\\n' 'native content worker exited' >&2\n"
                "exit 1\n",
                encoding="utf-8",
            )
            fake.chmod(0o700)
            result = corpus.run_public_journey(
                str(fake),
                {
                    "id": "worker-exit-fixture",
                    "scope": "public-read-only",
                    "timeoutMs": 1000,
                    "steps": [
                        {"action": "navigate", "url": "https://example.com"},
                        {"action": "observe"},
                    ],
                },
                root,
            )

        self.assertEqual(result["classification"], "product_bug")
        self.assertEqual(
            [(step["action"], step["status"]) for step in result["steps"]],
            [("navigate", "ok"), ("observe", "error")],
        )
        self.assertEqual(result["steps"][1]["errorKind"], "worker")
        self.assertEqual(result["errorKind"], "worker")
        self.assertIsNotNone(result["progressArtifact"])

    def test_public_report_never_retains_raw_error_or_final_url_suffix(self):
        with tempfile.TemporaryDirectory(prefix="glass-native-journey-redact-") as directory:
            root = Path(directory)
            fake = root / "fake-browser"
            fake.write_text(
                "#!/bin/sh\n"
                "cat <<'JSON'\n"
                '{"success":false,"finalUrl":"https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/img?token=URL_SENTINEL#FRAGMENT_SENTINEL",'
                '"steps":[{"index":0,"action":"navigate","status":"error","elapsedMs":2,"message":"private text ERROR_SENTINEL"}],'
                '"completed":0,"total":1}\n'
                "JSON\n"
                "exit 1\n",
                encoding="utf-8",
            )
            fake.chmod(0o700)
            result = corpus.run_public_journey(
                str(fake),
                {
                    "id": "redacted-report-fixture",
                    "scope": "public-read-only",
                    "timeoutMs": 1000,
                    "steps": [{
                        "action": "navigate",
                        "url": "https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/img",
                    }],
                },
                root,
            )

        serialized = json.dumps(result)
        self.assertEqual(
            result["finalUrl"],
            "https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/img",
        )
        self.assertEqual(result["steps"][0]["message"], "journey_step_error")
        self.assertNotIn("URL_SENTINEL", serialized)
        self.assertNotIn("FRAGMENT_SENTINEL", serialized)
        self.assertNotIn("ERROR_SENTINEL", serialized)

    def test_unexpected_final_path_cannot_be_reported_as_a_pass(self):
        with tempfile.TemporaryDirectory(prefix="glass-native-journey-final-url-") as directory:
            root = Path(directory)
            fake = root / "fake-browser"
            fake.write_text(
                "#!/bin/sh\n"
                "cat <<'JSON'\n"
                '{"success":true,"finalUrl":"https://developer.mozilla.org/en-US/docs/users/USER_SENTINEL",'
                '"steps":[{"index":0,"action":"navigate","status":"ok","elapsedMs":2}],'
                '"completed":1,"total":1}\n'
                "JSON\n",
                encoding="utf-8",
            )
            fake.chmod(0o700)
            result = corpus.run_public_journey(
                str(fake),
                {
                    "id": "unexpected-final-path-fixture",
                    "scope": "public-read-only",
                    "timeoutMs": 1000,
                    "steps": [{
                        "action": "navigate",
                        "url": "https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/img",
                    }],
                },
                root,
            )
        self.assertEqual(result["classification"], "inconclusive")
        self.assertEqual(result["errorKind"], "unexpected_final_url")
        self.assertEqual(result["finalUrl"], "unexpected_path")

    def test_public_report_projects_diagnostic_without_sources_or_page_text(self):
        with tempfile.TemporaryDirectory(prefix="glass-native-journey-image-diag-") as directory:
            root = Path(directory)
            fake = root / "fake-browser"
            fake.write_text(
                "#!/bin/sh\n"
                "cat <<'JSON'\n"
                '{"success":false,"finalUrl":"https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/img?token=URL_SENTINEL",'
                '"steps":[{"index":2,"action":"wait","status":"error","elapsedMs":8000,'
                '"message":"wait timed out for javascript_predicate ERROR_SENTINEL",'
                '"diagnostics":{"supported":true,"readyState":"complete","title":"TITLE_SENTINEL",'
                '"imageCount":1,"loadedCount":0,"sourcePresentCount":1,"incompleteWithSourceCount":0,'
                '"completeWithoutDimensionsCount":1,"visibleCount":0,"lazyHintCount":0,'
                '"decodeApiSupported":true,"images":[{"complete":true,"width":0,"height":0,"sourcePresent":true,'
                '"visible":false,"src":"https://private.example/SRC_SENTINEL","alt":"ALT_SENTINEL"}]}}],'
                '"completed":2,"total":3}\n'
                "JSON\n"
                "exit 1\n",
                encoding="utf-8",
            )
            fake.chmod(0o700)
            journey = {
                "id": "image-redaction-fixture",
                "scope": "public-read-only",
                "timeoutMs": 1000,
                "steps": [{
                    "action": "navigate",
                    "url": "https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/img",
                }],
            }
            result = corpus.run_public_journey(str(fake), journey, root)

        rendered = json.dumps(result)
        self.assertEqual(
            result["steps"][0]["imageDiagnostics"]["category"],
            "fetch_or_decode_failure_unresolved",
        )
        for sentinel in ("URL_SENTINEL", "ERROR_SENTINEL", "TITLE_SENTINEL", "SRC_SENTINEL", "ALT_SENTINEL"):
            self.assertNotIn(sentinel, rendered)

    def test_timeout_kills_and_reaps_the_owned_process_group(self):
        with tempfile.TemporaryDirectory() as directory:
            child_pid_path = Path(directory) / "child.pid"
            code = (
                "import signal, subprocess, sys, time; "
                "child = subprocess.Popen(['python3', '-c', 'import signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(30)']); "
                "open(sys.argv[1], 'w').write(str(child.pid)); "
                "signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(30)"
            )
            stdout, _, return_code, timed_out = corpus.run_process_group(
                [sys.executable, "-c", code, str(child_pid_path)],
                cwd=Path(directory),
                env=os.environ.copy(),
                timeout=1.0,
            )
            self.assertTrue(timed_out)
            self.assertNotEqual(return_code, None)
            child_pid = int(child_pid_path.read_text(encoding="utf-8"))
            dead = False
            for _ in range(100):
                stat_path = Path(f"/proc/{child_pid}/stat")
                try:
                    state = stat_path.read_text(encoding="utf-8").split()[2]
                except FileNotFoundError:
                    dead = True
                    break
                if state == "Z":
                    dead = True
                    break
                time.sleep(0.01)
            self.assertTrue(dead, f"timed-out child remains live; stdout={stdout!r}")

    def test_timeout_drains_pipes_after_the_direct_process_exits(self):
        with tempfile.TemporaryDirectory() as directory:
            child_pid_path = Path(directory) / "child.pid"
            code = (
                "import signal, subprocess, sys, time; "
                "child = subprocess.Popen(['python3', '-c', 'import signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(30)']); "
                "open(sys.argv[1], 'w').write(str(child.pid)); "
                "signal.signal(signal.SIGTERM, lambda *_: sys.exit(0)); time.sleep(30)"
            )
            _, _, return_code, timed_out = corpus.run_process_group(
                [sys.executable, "-c", code, str(child_pid_path)],
                cwd=Path(directory),
                env=os.environ.copy(),
                timeout=1.0,
            )
            self.assertTrue(timed_out)
            self.assertEqual(return_code, 0)
            child_pid = int(child_pid_path.read_text(encoding="utf-8"))
            stat_path = Path(f"/proc/{child_pid}/stat")
            if stat_path.exists():
                self.assertIn(stat_path.read_text(encoding="utf-8").split()[2], {"Z", "X"})


if __name__ == "__main__":
    unittest.main()
