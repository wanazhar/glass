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
current document and binds actions to that observation revision. Additional
local journeys open an observed image link and assert decoded dimensions plus
painted pixels, and use a disclosure and GET search form to reach and verify a
result page before checking browser history. A responsive duplicate-link
fixture exercises the native semantic API and batch runner: it hides the
small-screen copy, observes the page, and requests the `link` named `library`.
When the target is outside the response's bounded target window, `clickObserved`
performs a unique role and case-insensitive accessible-name-fragment lookup
against a fresh native semantic source
snapshot and dispatches its reference with that revision. Repeated visible
links collapse to one action only when each candidate has the same
privacy-safe destination fingerprint; different destinations remain an
ambiguity error.
The repository's broader authored fixture coverage remains grouped under
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

The public entries cover a deep MDN reference, a Wikipedia article that
follows an observed link to a second article, and a GitHub repository journey
that follows an observed link into the Rust repository's `library` directory.
The GitHub journey checks repository identity, follows the observed `library`
link into the Rust repository, then asserts the subpage URL and directory
content. On 25 October the native run passed all six steps in 50.62 seconds.
The responsive duplicate link is resolved from fresh semantic evidence and
dispatched at that observation revision; no CSS selector or node ID is baked
into the public journey. Earlier click-deadline failures remain historical
measurements and do not describe the latest run.

The Svelte docs journey uses the deep `/docs/svelte/svelte-reactivity` page,
asserts its page title through the native semantic snapshot, scrolls the
observed viewport, and takes a fresh observation afterward. On 25 October it
passed all six steps in 80.21 seconds, including the accepted 400-pixel scroll.
The SSR article contains about 20,000 characters and 18 headings. The run is a
functional pass with substantial latency: navigation took 73.19 seconds;
measured stage totals included 47.61 seconds of page-script evaluation, 33.36
seconds for document bootstrap, 16.85 seconds computing styles, and 14.73
seconds building layout. These stage timings overlap and are not additive.
A shorter Svelte overview page had no scrollable extent and is not used as a
scroll scenario. The local nested-scroll fixture separately passes through a
revision-bound `scrollTargetObserved` selection. A missing selector reports up
to eight observed target tags and bounded labels to make target selection
evidence inspectable without dumping the page.
The latest bounded Wikipedia
journey on 25 October did not reach its first observation: navigation hit its
120-second deadline. It ran 12 host-view installs, with the slowest at 8.03 s,
under the unchanged 15-second per-install limit. Lazy image-pixel decoding
reduced the image-resource host-view phase from 6.90 s to 0.0007 s in the
measured run. The remaining measured work included 27.8 s computing 16,053
elements' styles, 20.5 s measuring text widths, 23.5 s building the initial
layout tree, and 47.5 s in the slowest classic page script. Total page-script
execution reached 60.8 s. This is a product deadline failure, not a Wikipedia
compatibility pass; repeated style/layout and page-script work remain open.
After this public run, a bounded per-layout text-width cache was added for exact
short LTR text and default shaping options. A synthetic 64-span fixture measured
127 cache hits, 769 direct text-width calls, and a 44.3 ms layout. A later
bounded Wikipedia run with the cache enabled exceeded the runner's 120-second
outer deadline and returned inconclusive with no step result, so it is not a
site pass. Its private trace recorded 7,397 cache hits, 54,926 direct text-width
measurements taking 12.995 s, 12 host-view installs (maximum 6.60 s), and nine
layout builds totaling 15.36 s. Page-script execution totaled 45.64 s, with a
33.72-second maximum single script; computed style took 30.11 s. A prior run
measured 20.46 s for text widths, but the public DOM varies between runs, so
these are not a controlled speedup comparison. A previous run failed at the
15-second host-view deadline and an earlier run raised a QuickJS null exception
during element construction; those are separate outcomes. A local dense fixture with 5,500 anchors and 1,000 styled spans
passes, but does not reproduce the public page's full style and script
workload. The native image journey passes locally with decode, intrinsic
dimensions, and painted-pixel assertions; the local disclosure/search/history
and observed nested-scroll journeys also pass. The Wikipedia journey declares
navigation, observation, and observed subpage navigation; the current attempt
failed before observation. A single native MDN deep-reference journey passed,
though an earlier JavaScript URL predicate timed out and was replaced with a
bounded URL-prefix wait. These are individual site results, not universal
compatibility. Run these scenarios only through the native backend and record
network denial, challenge pages, external instability, unsupported
capabilities, and product bugs as distinct outcomes. A successfully fetched
video poster is not video playback: the native engine currently reports video
decoding as unsupported.

## Native video capability boundary

The current media loader admits bounded encoded bytes and exposes MIME and,
for WAV, duration metadata. It does not demux video containers, decode video
frames, schedule an audio/video clock, or render an audio output stream. The
native `HTMLVideoElement.canPlayType()` therefore returns the empty string and
`play()` rejects with `NotSupportedError`; poster fetch/paint is an independent
image path. Keep these answers truthful until every advertised type has a
working decoder and playback path.

A real implementation needs separate bounded stages: (1) resource policy and
byte admission; (2) container demux into timestamped packets; (3) codec decode
into bounded video frames and audio samples; (4) a monotonic media clock with
pause, seek, rate, readiness and ordered media events; and (5) frame invalidation
and painting through the native display-list path plus an actual audio sink.
The content process should own the media session and communicate only
generation-tagged state/frame updates to its document; stale navigation must
cancel decoding and discard late frames. Enforce caps for encoded bytes,
dimensions, decoded frame bytes, queued frames, and work deadlines, with
back-pressure rather than unbounded buffering. Tests must distinguish poster
painting from decoded frame progression and assert seeking, pause, time advance,
and audio capability independently.

The native feature currently depends on still-image decoders (PNG, JPEG, GIF,
and WebP) and contains no video/audio decoder or sink dependency. A system
FFmpeg/GStreamer binding would add native-library and package availability
requirements across release targets; an external command would add process and
sandbox lifecycle concerns. A Rust demux/decoder stack avoids that system
dependency but still needs an explicit supported-container/codec matrix,
license review, and a separate audio output design. The implementation must
choose and package a narrow initial matrix before reporting support; it must
not infer support from a recognized MIME type or successful metadata parse.

## Coverage map

| Journey surface | Evidence source | Current status |
|---|---|---|
| Multi-page navigation and history | `versioned_native_journey_corpus_runs_local_navigation_and_scroll_journey` | Pass: observed link, detail content, back/forward, and a fresh revision-bound nested-scroll target |
| Observed nested scroll container | Versioned local journey | Pass: bounded `scrollTargets` expose revision-bound references; `scrollTargetObserved` requires an explicit name or tag selector and reports compact available targets on a miss |
| Image gallery to full image | `versioned_native_image_journey_opens_and_paints_full_image` | Local fixture asserts observed link navigation, decode completion, intrinsic dimensions, and actual raster pixels |
| Search, disclosure, and GET form | `versioned_native_disclosure_search_form_and_history_journey` | Local fixture asserts disclosure state, typed query, result URL/content, and back navigation |
| Responsive duplicate links | `versioned_native_journey_selects_observed_responsive_duplicate_by_region` | Pass: region-scoped selector resolves one of two identical destination links and follows its revision-bound target to asserted content |
| Public repository subpage | `public-github-repository-and-observed-directory` | Pass on 25 October: observed repository link, subpage URL, and directory content; 6/6 steps in 50.62 s |
| Public documentation page scroll | `public-svelte-documentation-page-scroll` | Pass on 25 October: Svelte reactivity page title, accepted viewport scroll, and post-scroll observation; 6/6 in 80.21 s. Page-script execution remains 47.61 s in this run. |
| Video decoding and playback | `native_video_reports_decoder_absence_and_does_not_claim_playback` | Explicitly unsupported; poster rendering is separate |
| Dynamic disclosure, search form, and result history | `versioned_native_disclosure_search_form_and_history_journey` | Pass: click an observed disclosure control, verify hidden-state/content update, type a GET query, verify result URL/content, and go back |
| Menus, frames, SPA routes, and additional form patterns | Focused authored fixture tests under `tests/native_engine_tests/` | Add journeys where a measured gap is demonstrated |

This corpus does not assert universal compatibility with public websites.
