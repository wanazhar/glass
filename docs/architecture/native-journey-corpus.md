# Native browser journey corpus

The versioned corpus at
[`crates/glass-browser/tests/fixtures/native-journeys-v1.json`](../../crates/glass-browser/tests/fixtures/native-journeys-v1.json)
defines repeatable native-browser journeys. It separates deterministic local
fixtures from optional public, read-only observations. Public journeys must
use bounded timeouts, avoid accounts and state-changing submissions, and
record blocked or denied responses separately from product failures. Reports
should retain per-step duration, backend, capability outcome, typed error,
final URL/title, and references to private diagnostic artifacts. They must not
save full third-party pages, credentials, cookies, or personal data.

The local navigation journey is executed by
`versioned_native_journey_corpus_runs_local_navigation_and_scroll_journey` in
the native integration suite. It resolves link and scroll targets from the
current document and binds actions to that observation revision. The
repository's broader authored fixture coverage remains grouped under
`tests/native_engine_tests/`; this corpus is an acceptance entry point, not a
replacement for those focused regressions.

Run the optional network journeys with
`python3 scripts/run-native-journey-corpus.py --public`. The runner uses the
native `glass-browser batch` interface, launches a fresh incognito session per
journey, performs no retries, and writes a redacted JSON report with one-run
and per-step timings and outcomes. The default report is placed in the system
temporary directory with owner-only permissions; pass `--output PATH` to
choose a different private location. A failed journey returns a nonzero exit
code while preserving the report.

The first public entries cover a deep MDN reference and a Wikipedia article
that follows an observed link to a second article. The local journey passes.
One native MDN deep-reference run navigated and observed successfully (47.6 s
navigation, 3.0 s observation), though the earlier JavaScript URL predicate
timed out and was replaced with a bounded URL-prefix wait. The current
Wikipedia run does not pass: native host-view construction reaches QuickJS's
256 MiB heap ceiling at element 1,640 while building an approximately 16k-node
article snapshot. Its typed result is `product_bug / quickjs_out_of_memory`;
the article journey cannot continue until host-view retained memory is reduced.
Do not report the Wikipedia homepage or the partial MDN run as deep-journey
success. Run these scenarios only through the native backend and record
network denial, challenge pages, external instability, unsupported
capabilities, and product bugs as distinct outcomes. A successfully fetched
video poster is not video playback: the native engine currently reports video
decoding as unsupported.

## Coverage map

| Journey surface | Evidence source | Current status |
|---|---|---|
| Multi-page navigation and history | Versioned local journey | Executed by native integration test |
| Observed nested scroll container | Versioned local journey | Explicit `scrollTarget`; arbitrary-pane fallback is unsupported |
| Images and capture | Existing `style_layout_rendering/images_and_svg` tests; public article scenario | Local fixture coverage exists; public article did not reach interaction because host-view construction ran out of memory |
| Video decoding and playback | `native_video_reports_decoder_absence_and_does_not_claim_playback` | Explicitly unsupported; poster rendering is separate |
| Menus, forms, frames, dynamic updates, SPA routes | Focused authored fixture tests under `tests/native_engine_tests/` | Continue to add journeys where gaps are demonstrated |

This corpus does not assert universal compatibility with public websites.
