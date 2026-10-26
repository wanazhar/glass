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
ambiguity error. When a fragment matches multiple visible targets, callers may
provide `nameEquals` to require a case-insensitive exact accessible name from
that same fresh observation; the resolver still reports ambiguity if
multiple exact-name targets point to different destinations. The public
Wikipedia journey uses this selector after its article observation exposed
multiple links containing “computer science.”
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

`timeoutMs` bounds an individual navigation step. A journey may set
`runnerTimeoutMs` for its total owned process-group deadline (1–600 seconds);
the default remains `timeoutMs + 10 seconds`. This distinction matters for
multi-step pages whose navigation succeeds near its limit but still need time
for observation and semantic actions. The runner keeps completed step results
and marks an interrupted active step when the outer deadline expires, then
terminates and reaps only the process group it launched. The Wikipedia
article-to-subpage journey uses a 360-second outer bound for its seven
read-only steps while retaining its 120-second per-navigation budget. This
gives the complete journey room to run; it does not assert that the native
engine can meet that deadline or convert an interrupted step into a pass.

The public entries cover a deep MDN reference, a Wikipedia article that
asserts the first article title, follows the freshly observed
“Computer science” link by accessible role and name, then checks the second
article URL and title, and a GitHub repository journey
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
video poster is not video playback; playback evidence must show decoded frame
progression and pause/seek behavior.

## Wikipedia style-cache follow-up (2026-10-26)

Two bounded native article-to-subpage attempts after the selector and text
width cache work remained inconclusive. The first completed navigation in
96.343 seconds and timed out during its first observation at the 130.978
second outer deadline. Its trace showed 55,135 style-compute calls totaling
34.58 seconds and six document-wire style projections totaling 37.94 seconds.
Review of the same trace found 48 full style invalidations, including 36
decoded image arrivals. Commit `b612c105` narrows image and video-poster
arrival invalidation to layout and script projections; a local 36-image test
proves that each new image updates intrinsic geometry while retaining cached
computed styles and custom properties.

A second run against that source completed navigation in 95.697 seconds, then
its first observation was interrupted by the same outer deadline. The trace
recorded 36 resource-only invalidations and 10 full style invalidations; style
computation measured 31.88 seconds across 52,028 calls, and document-wire
style projection took 34.85 seconds. The preceding trace measured 34.58 and
37.94 seconds respectively, but the live response varies between runs, so
this is not a controlled end-to-end comparison. Neither attempt observed the
article or reached the semantic subpage; Wikipedia remains an open native
acceptance gap. Reports contain step outcomes and artifact references without
retaining page dumps or user data.

The matching local style workload uses 4,096 rules across 16,006 DOM nodes,
with common tag/class rules plus frequent and rare class anchors. Candidate
lookup visits 16,928,000 rule candidates in 311–336 ms, and a complete cached
style pass for 8,004 elements takes 1.98–2.03 seconds. The candidate set
matches a full selector scan for sampled anchors, and reusable scratch keeps
its warmed capacity. A trial hash-map index was slightly slower (317 ms and
2.10 seconds), so the existing ordered index remains. This local profile helps
separate candidate lookup from the more expensive rule matching; it does not
explain all of the public page cost or establish compatibility.

## Native video capability boundary

The native engine currently supports a narrow silent-video path for WebM files
with a VP8 track. `matroska-demuxer` demuxes the container and the pure Rust
`zvidlib-vp8` decoder produces RGBA frames; `canPlayType()` reports `maybe` for
`video/webm` and `video/webm; codecs="vp8"`. The native media clock advances,
pauses, seeks, and selects the decoded frame painted by the display list. An
authored local two-color clip verifies that seeking and elapsed playback show
the blue second frame. This is one deterministic local capability result, not
a claim of broad public-site compatibility.

The decoder is bounded: encoded media remains capped at 16 MiB; VP8 dimensions
at 640 by 640; decoded data at 2 MiB per resource and 4 MiB per document; and
retained frames at 60 per resource and 120 per document. Media outside those
caps or without a VP8 track is not advertised as playable. Other containers
and codecs, audio decoding/output, captions, and full media Web IDL behavior
remain unsupported. Poster fetch/paint remains an independent image path.

The decoder dependencies are optional through `glass-browser-core/native-engine`.
[`zvidlib-vp8`](https://docs.rs/zvidlib-vp8/0.5.0) is MIT-licensed and built
with default features disabled, selecting its software decoder across supported
targets. [`matroska-demuxer`](https://docs.rs/matroska-demuxer/0.8.1) is
available under Zlib, MIT, or Apache-2.0. The engine does not invoke a system FFmpeg
binary, link libvpx, or claim an audio output device. The fixture file is a
locally generated two-frame test pattern; FFmpeg is only a fixture-generation
tool and is not a runtime dependency.

An alternative Rust VP8/VP9 candidate inspected earlier was
[`media-codec-vpx`](https://crates.io/crates/media-codec-vpx)
is MIT/Apache-2.0 licensed, but its `media-codec-vpx-sys` build script uses
`pkg-config` to require system `libvpx` 1.15.2 or later and runs `bindgen` over
the system headers. It does not bundle the decoder. The 25 October development audit found no
`vpx.pc` metadata, and the existing Linux/macOS/Windows release matrix does not
provision libvpx development/runtime packages. Adding the crate as-is would
therefore make native-engine builds depend on an undeclared per-platform
library install; it would still leave WebM demuxing, bounded frame delivery,
playback scheduling, and audio or explicit silent-video policy to implement.
This candidate is not yet a validated distributable route.

### Current dependency and output audit

`symphonia`, FFmpeg bindings, GStreamer, `cpal`, and `rodio` are not part of
this narrow video path. The system `ffmpeg` executable available in this
development environment reports a GPL build with GPL-enabled codecs,
including `libx264`; it is not a runtime dependency or redistributable decoder
choice. FFmpeg's [license guidance](https://www.ffmpeg.org/legal.html)
distinguishes its LGPL build from optional GPL components. The chosen VP8
software decoder and WebM demuxer are Rust crates with permissive licenses and
need no system multimedia package.

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
| Silent WebM/VP8 video decoding and playback | `native_webm_vp8_play_pause_seek_paints_decoded_video_frame` plus `native_webm_vp8_decodes_bounded_real_rgba_frames` | Pass: local authored clip verifies two decoded colors, 1 s metadata, capability, elapsed playback, pause, seek, and painted frame |
| Dynamic disclosure, search form, and result history | `versioned_native_disclosure_search_form_and_history_journey` | Pass: click an observed disclosure control, verify hidden-state/content update, type a GET query, verify result URL/content, and go back |
| Menus, frames, SPA routes, and additional form patterns | Focused authored fixture tests under `tests/native_engine_tests/` | Add journeys where a measured gap is demonstrated |

This corpus does not assert universal compatibility with public websites.
