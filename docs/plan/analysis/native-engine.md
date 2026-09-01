# Native engine Phase 1/Phase 2, initial Phase 3, and runtime integration analysis

Status: Active implementation analysis for issue #40; Phase 0/1 and the first
Phase 2 semantic/action/form-control/parser slices, the initial Phase 3
presentation/layout slices, and the 008/009 runtime and input checkpoints are
committed locally, including the bounded 010 display-list seed and the bounded
011 software-surface seed, 012 style-inheritance seed, 013 paint-clipping
seed, 014 uniform solid-border paint, 015 bounded PNG capture, 017
viewport-scroll, and 018 side-specific-border slices. The 018 slice is
complete locally and its checkpoint evidence is recorded on issue #40. The
019 bounded dashed/dotted-border slice is complete locally and its checkpoint
evidence is recorded on issue #40. The 020 bounded physical border-radius
slice is complete locally and its checkpoint evidence is recorded on issue #40.
The 021 bounded inline line-placement slice is complete locally and its
checkpoint evidence is recorded on issue #40.
The 022 bounded fixed line-height slice is complete locally and its checkpoint
evidence is recorded on issue #40.
The 023 bounded direct-text-flow slice is complete locally and its checkpoint
evidence is recorded on issue #40.
The 024 bounded word-aware text-wrapping slice is complete locally and its
checkpoint evidence is recorded on issue #40.
The 025 bounded physical box-edges slice is complete locally; its complete gate
evidence is recorded below and its issue #40 checkpoint is updated from the
same commit.
The 026 bounded whitespace-boundary slice is complete locally; its complete
gate evidence is recorded below and its issue #40 checkpoint is updated from
the same commit.
The 027 bounded overflow hit-test/projection slice is complete locally; its
complete gate evidence is recorded below and its issue #40 checkpoint is
updated from the same commit.
The 028 bounded unsupported-CSS-diagnostics slice is complete locally; its
complete gate evidence is recorded below and its issue #40 checkpoint is
updated from the same commit. It makes ignored selectors, properties, values,
and malformed CSS observable through a bounded revisioned Rust surface without
changing stable backend evidence or the existing deterministic fallback
behavior.
The 029 bounded pixel-golden-capture slice is complete locally; its complete
gate evidence is recorded below and its issue #40 checkpoint is updated from
the same commit. It certifies the existing logical surface and PNG path with
one complete fixed fixture golden without expanding screenshot evidence or
renderer scope.
The 030 bounded descendant-selector slice is complete locally; its complete
gate evidence is recorded below and its issue #40 checkpoint is updated from
the same commit. It extends the narrow compound-selector grammar through
bounded descendant ancestor matching without claiming general CSS selector
conformance.
The 031 bounded-overflow-clip slice is complete locally; its complete gate
evidence is recorded below and its issue #40 checkpoint is updated from the
same commit. It accepts the non-scrolling `overflow: clip` value through the
existing bounded rectangular clip path without adding nested scrolling or
general CSS overflow conformance.
The 032 bounded-hard-line-breaks slice is complete locally: visible `<br>`
elements advance the existing integer inline-flow cursor without creating
synthetic semantic/layout/paint nodes or claiming general inline formatting
conformance. Its complete gate evidence is recorded below and its issue #40
checkpoint is updated from the same commit.
The 033 bounded-pre-line-breaks slice is complete locally: inherited
`white-space: pre-line` turns bounded source line-feed and carriage-return
boundaries into the same hard flow breaks while retaining space collapsing and
the no-general-CSS boundary. Its complete gate evidence is recorded below and
its issue #40 checkpoint is updated from the same commit.
The 034 bounded-preformatted-whitespace slice is complete locally: inherited
`white-space: pre` retains literal fixed-cell source whitespace, turns bounded
LF/CR/CRLF boundaries into the same hard flow breaks, and does not soft-wrap
preformatted segments. Its contract explicitly leaves tab stops, wide-line
reflow/scrolling, text alignment, font metrics, and general CSS whitespace
conformance outside the native claim; its complete gate evidence is recorded
in `docs/plan/tasks/native-engine-034.md` and its issue #40 checkpoint is
updated from the same commit.
The 035 bounded-pre-wrap-whitespace slice is complete locally: inherited
`white-space: pre-wrap` retains literal fixed-cell source whitespace and hard
LF/CR/CRLF breaks while splitting source runs at deterministic fixed-cell
soft-wrap boundaries. Its contract leaves browser line breaking, tab stops,
font metrics, shaping, baselines, bidi, justification, `break-spaces`,
`nowrap`, and general CSS whitespace conformance outside the native claim; its
complete gate evidence is recorded in `docs/plan/tasks/native-engine-035.md`
and its issue #40 checkpoint is updated from the same commit.
The 036 bounded-base64-data-url slice is complete locally: standard padded
base64 `data:text/html` payloads decode through the existing local resource
loader and real navigation/dispatcher path under a derived encoded bound and
the existing decoded UTF-8 document limit. Its contract keeps URL-safe or
whitespace-tolerant encodings, percent-encoded base64, subresources, network,
filesystem, script, storage, and arbitrary schemes outside the native claim;
its complete gate evidence is recorded in
`docs/plan/tasks/native-engine-036.md` and its issue #40 checkpoint is updated
from the same commit.
The 037 bounded-fragment-navigation-history slice is complete locally: raw
fragments are separated from local resource lookup, same-resource navigation
is document-preserving and revisioned, and explicit Rust back/forward traversal
is implemented without inventing a transport-level history operation. Its
completion evidence is recorded in `docs/plan/tasks/native-engine-037.md` and
issue #40.
The 038 bounded-local-link-activation slice is complete locally: semantic local
anchor clicks route through the existing action and navigation owner while
relative, remote, and other unsupported href destinations remain rejected. Its
completion evidence is recorded in `docs/plan/tasks/native-engine-038.md` and
issue #40.
The 039 bounded-fragment-target-scroll slice is complete locally: exact visible
local `id` targets position the root viewport at a bounded document-space top,
and each history entry retains its saved root-scroll offset for same- and
different-resource traversal restoration. Its implementation and validation
evidence are recorded in `docs/plan/tasks/native-engine-039.md` and issue #40.
The 040 bounded-relative-local-links slice is complete: semantic links now
resolve same-host fixture-relative URLs while preserving the existing
parse-before-commit, anchor-scroll, and failure-atomicity boundaries. Its
contract and validation evidence are recorded in
`docs/plan/tasks/native-engine-040.md`.
The 041 bounded-percent-decoded-fragment-target slice is complete locally:
local fragment navigation decodes bounded UTF-8 percent escapes before exact
visible `id` matching while preserving the existing duplicate-safe scroll,
history, and failure behavior. Its contract and validation evidence are
recorded in `docs/plan/tasks/native-engine-041.md` and issue #40.
The 042 bounded-legacy-name-fragment-target slice is complete locally: it adds
a unique exact `<a name>` fallback only when no matching `id` exists, while
preserving the existing decoded fragment, layout, scroll, history, and
duplicate-safe boundaries. Its contract and validation evidence are recorded
in `docs/plan/tasks/native-engine-042.md` and issue #40.
The 043 bounded-text-fragment-target slice is complete locally: it matches a
simple one-run `#:~:text=start` or `#:~:text=start,end` request after per-term
bounded UTF-8 decoding while preserving the existing ID/name precedence,
layout, scroll, and history boundaries. Its complete gate evidence is recorded
below, in `docs/plan/tasks/native-engine-043.md`, and on issue #40.
The 044 bounded-text-fragment-affix slice is complete locally: it adds exact
same-run prefix/suffix affixes around the existing matcher, with the same
raw-comma parsing, per-term UTF-8 decoding, scroll, history, and fail-closed
boundaries. Its complete gate evidence is recorded below, in
`docs/plan/tasks/native-engine-044.md`, and on issue #40. The 045 bounded
root-horizontal-scroll slice is complete locally: it derives measured overflow
width, independently clamps x/y, and reuses the existing viewport, hit-test,
display-list, raster, and history owners. Its complete gate evidence is
recorded below, in `docs/plan/tasks/native-engine-045.md`, and on issue #40.
The 046 bounded-nowrap-whitespace slice is complete locally: it accepts
inherited and inline `white-space: nowrap`, collapses supported whitespace
through the existing fixed-cell policy, disables soft wrapping, and reuses the
measured root horizontal scroll/projection path. Its complete gate evidence is
recorded below, in `docs/plan/tasks/native-engine-046.md`, and on issue #40.
The 047 bounded-inherited-line-height slice is complete locally: it propagates
the existing positive-pixel `line-height` through the DOM style walk while
preserving explicit child declarations and height precedence. Its complete
gate evidence is recorded in `docs/plan/tasks/native-engine-047.md` and issue
#40. The 048 bounded-clip-aware-root-overflow slice is complete locally: it
measures text contribution through the existing paint/projection clip owner so
fully clipped text cannot create a false root horizontal scroll range. Its
complete gate evidence is recorded in `docs/plan/tasks/native-engine-048.md`
and issue #40. The 049 bounded-axis-specific-overflow slice is complete
locally: it cascades bounded `overflow-x`/`overflow-y` clips independently
through the same owner. Its complete gate evidence is recorded in
`docs/plan/tasks/native-engine-049.md` and issue #40. The completed
dependency-ordered task is `native-engine-050`: bounded physical min/max
width/height constraints through the existing box-model owner. The completed
`native-engine-051` task adds bounded CSS opacity groups through transparent
display-list layers and software compositing. No general stacking, transform,
filter, animation, or browser compositor parity is implied. The completed
`native-engine-052` task adds bounded inherited `text-align` for fixed-cell
direct text and supported inline flow, with no logical-direction or
justification parity. The completed dependency-ordered `native-engine-053`
task adds bounded functional `rgba(R, G, B, A)` alpha parsing for background,
border, and text colors through the existing `NativeColor` and software
source-over owners. It does not imply CSS Color 4, color-space, or
color-management parity. The completed dependency-ordered `native-engine-054`
task adds
bounded inherited `text-decoration: none|underline` through immutable text
commands and fixed-cell software replay. It does not imply font metrics,
decoration propagation, or browser text-paint parity. Its implementation and
validation evidence are recorded in the task file and issue #40. The completed
dependency-ordered `native-engine-055` task adds bounded inherited ASCII
`text-transform: none|uppercase|lowercase` during fixed-cell layout so
wrapping, text-fragment matching, display-list projection, and root-overflow
measurement consume the same presentation text. Semantic source text remains
unchanged; Unicode case mapping, locale behavior, and font-specific glyph
metrics remain outside the boundary. Its implementation and validation
evidence are recorded in the task file and issue #40. The completed
dependency-ordered `native-engine-056` task adds bounded non-negative
fixed-pixel `text-indent` to the first line of block containers, clamps the
effective value to retain one fixed cell, and keeps inline/`display:contents`
text on the containing block's flow. Its implementation and local validation
evidence are recorded in the task file and issue #40; remote CI remains
pending until this branch is pushed.
The completed dependency-ordered `native-engine-057` task adds bounded inherited
non-negative fixed-pixel `word-spacing` to the existing fixed-cell text-flow
owner. Collapsed ASCII separator spaces and literal ASCII spaces in the
supported preformatted modes receive the extra advance before wrapping,
fragment projection, alignment, display-list generation, raster replay, hit
testing, and root-overflow measurement. Negative, relative, percentage,
keyword, Unicode-whitespace, and browser word-boundary behavior remain
outside the boundary. Its implementation and local validation evidence are
recorded in the task file and issue #40; remote CI remains pending until this
branch is pushed.
The completed dependency-ordered `native-engine-058` task adds bounded inherited
non-negative fixed-pixel `letter-spacing` to every rendered fixed-cell
character in each emitted fragment. Its measured advance composes with
`word-spacing` on ASCII spaces before wrapping, preformatted chunking,
fragment projection, alignment, display-list generation, raster replay, hit
testing, and root-overflow measurement. Fragment and line boundaries remain
hard boundaries; browser pair-boundary, Unicode, font-metric, negative,
relative, percentage, `normal`, and conformance semantics remain outside the
boundary. Its implementation is committed locally as `cb191a3`; the task file
records the local validation evidence, and remote CI remains pending until this
branch is pushed.

The completed dependency-ordered `native-engine-059` slice adds inherited
`font-weight: normal|bold|400|700` to the fixed-cell text presentation path.
`normal`/`400` retain the current glyph replay and `bold`/`700` apply a
deterministic one-pixel horizontal glyph dilation without changing advances,
layout, semantics, hit testing, overflow, or text-fragment coordinates.
Real font selection, metrics, shaping, variable weights, and browser text
rendering parity remain outside the boundary. The implementation is committed
locally as `21fcff5`; the task file records the local validation evidence, and
remote CI remains pending until this branch is pushed.

The completed dependency-ordered `native-engine-060` slice adds inherited
`font-style: normal|italic` to the fixed-cell text presentation path.
`normal` retains the current glyph replay and `italic` applies a deterministic
bounded row-dependent horizontal shear through the existing clipped software
rasterizer. Bold dilation, underline, spacing, opacity, scrolling, and capture
compose through the same immutable text command; advances, layout, semantic
text, hit testing, overflow, and text-fragment coordinates remain unchanged.
Oblique forms, angles, font selection/loading/metrics, shaping, anti-aliasing,
and browser text-rendering parity remain outside the boundary. The
implementation is committed locally as `2992eb8`; the task file records the
local validation evidence, and remote CI remains pending until this branch is
pushed.

The completed dependency-ordered `native-engine-061` slice adds inherited
`word-break: normal|break-all` to the bounded collapsed fixed-cell flow path.
`normal` retains word-aware wrapping; `break-all` permits deterministic
character-boundary splitting for every collapsed word while preserving the
existing separator, spacing, fragment, overflow, and semantic owners. `pre`,
`pre-wrap`, and `nowrap` retain their established behavior. Unicode
line-breaking, grapheme policy, hyphenation, `overflow-wrap`, bidi, writing
modes, font metrics, and browser conformance remain outside the boundary. The
design is `14d7fc4`, the implementation is `479f3a3`, and the documentation
closeout is `433d6fd`; local validation evidence is recorded in the task file
and remote CI remains pending until this branch is pushed.

The completed dependency-ordered `native-engine-062` slice adds local
`text-overflow: clip|ellipsis` to the bounded single-line fixed-cell path.
`clip` retains the existing full visual run under a horizontal overflow clip;
eligible `ellipsis` blocks replace an overflowing suffix with a
spacing-aware fixed-cell ASCII `...` marker while preserving the full semantic
source text. The implementation is restricted to one direct text child in a
rendered `nowrap` block with finite horizontal clipping; multi-line
truncation, nested inline formatting, Unicode ellipsis behavior, and browser
conformance remain outside the boundary. The design is `7e488aa`, the
implementation is `e4c5bb1`, and local validation evidence is recorded in the
task file; remote CI remains pending until this branch is pushed.

The active dependency-ordered `native-engine-063` design adds inherited
`vertical-align: baseline|top|middle|bottom` to the existing fixed-cell
inline-flow line-item owner. `baseline` preserves the current top-origin
behavior; `top`, `middle`, and `bottom` apply bounded integer offsets within
the existing line box and move an inline item's boxes and text artifacts
together. Font metrics, typographic baselines, lengths, percentages, bidi,
writing modes, ruby, table-cell alignment, and browser conformance remain
outside the boundary. The contract is recorded in
`docs/plan/tasks/native-engine-063.md`; implementation has not started.

Issue [#40](https://github.com/wanazhar/glass/issues/40) is the authority. The
current delivery is a Phase 0/Phase 1 kernel plus bounded Phase 2 semantic
DOM/interaction slices and initial Phase 3 presentation slices, not an attempt
to implement a complete browser in one change.

## Baseline and constraints

The current checkout has exactly two installable crates. The native engine
must stay inside `glass-browser`, remain default-off, and add no dependency in
these slices. Chromium/CDP remains the production path; native selection is
explicit-only.

The normal browser-free platform matrix uses `--no-default-features`; a
dedicated Linux `Native engine core` job owns the explicit `native-engine`
feature checks, tests, and strict Clippy. The two jobs use separate cache keys
so experimental compilation does not become an unobserved default-build cost.

Baseline captured on the working machine before the change:

```text
command: /usr/bin/time -f 'elapsed_seconds=%e peak_rss_kb=%M' \
  cargo check -p glass-browser --lib --locked
result: exit 0
elapsed_seconds: 11.22
peak_rss_kb: 1,132,708
```

This is a local observation, not a cross-platform budget. The post-change
verification records default and native-feature checks separately.

Post-change warm checks after the updated target was built:

| Check | Elapsed | Peak RSS | Scope |
|---|---:|---:|---|
| `cargo check -p glass-browser --lib --locked` | 0.26 s | 81,004 KiB | default features |
| `cargo check -p glass-browser --features native-engine --locked` | 0.61 s | 81,056 KiB | native feature |

These are warm incremental checks on this Linux host, not clean-build or
cross-platform claims. The native feature remains heavier to compile than the
default path only because it adds the Phase 1/Phase 2 module set; it adds no
external dependency. A future performance task should record clean builds, a
real edit touching the native module, and target-directory growth separately.

## Module decomposition

| Module | Owns | Inputs | Outputs | Dependencies |
|---|---|---|---|---|
| `native_engine::config` | public startup configuration, hard limits, and bounded local URL policy | URLs, viewport, fixtures, limits | validated `NativeEngineConfig` and local URL helpers | `url`, typed native error |
| `native_engine::lifecycle` | lifecycle state | transitions | `New`, `Running`, `Closed` | none |
| `native_engine::scheduler` | logical clock and bounded ordered tasks | task kind, delay | deterministic task IDs/order | native limits |
| `native_engine::history` | current local history and per-entry root scroll state | committed URL/revision/scroll offset | bounded entries/current index | native limits + layout point |
| `native_engine::origin` | Phase 1 origin placeholder | loaded URL | opaque origin | none |
| `native_engine::resource_loader` | fixture/data/about resource boundary | validated URL | bounded local HTML resource | `url`, config fixtures |
| `native_engine::css` | bounded selector/rule parsing, display/visibility presentation, inherited color, positive-pixel line-height, inherited physical text alignment, pixel dimensions, physical solid/dashed/dotted borders, circular border radii, physical padding/margin edges, local opacity alpha, inherited `font-weight:normal|bold|400|700`, inherited `font-style:normal|italic`, and inherited `word-break:normal|break-all` | style text, inline style, native element attributes, ancestor styles | deterministic computed presentation values | native DOM element surface |
| `native_engine::layout` | viewport-bounded block/inline normal-flow geometry, bounded outer/content box model, side-specific border insets, rounded-box metadata, preflight inline line placement, inherited fixed line-height floors, direct-text fragments, whitespace-boundary flow, source-order paint entries, aligned line-item ranges, opacity group boundaries, root scroll projection, rounded point hit testing, and bounded inherited word-break wrapping | DOM, computed presentation, viewport, scroll offset | document-space layout boxes/text fragments, paint order, scroll metadata, and deterministic hit target | native DOM + CSS presentation |
| `native_engine::paint` | revisioned clear/fill/text-fragment/physical-border display-list derivation, bounded rounded paint masks, source-order entries, opacity group markers, ancestor clips, and scroll metadata | current layout, bounded computed colors/text/borders/radii/opacity/font presentation, and overflow presentation | immutable document-space display-list commands | native DOM + layout |
| `native_engine::raster` | bounded logical RGBA surface replay for fills, text, rounded solid/dashed/dotted borders, nested opacity layers, PNG encoding, and viewport translation | immutable display-list commands and scroll offset | immutable software surface or bounded PNG bytes | native display list + existing `png` dependency |
| `native_engine::dom` | arena DOM, semantic projection, and bounded control/form mutation | HTML source, locators, and limits | generational nodes/document evidence/effects | native limits |
| `native_engine::interaction` | action/effect types and bounded effect records | semantic action and event kind | revisioned interaction metadata | native DOM IDs |
| `native_engine::engine` | sole mutable page-state coordinator | lifecycle/navigation/action requests | snapshots/context/history/effects | all engine modules |
| `browser::native_backend` | semantic adapter/profile | backend requests | typed backend responses/errors | engine + `browser_backend` |
| `browser::runtime` | explicit native session construction | `NativeEngineConfig`, runtime choice | initialized `BrowserRuntimeSession` | backend factory + dispatcher |
| `cli::runner` | feature-gated native one-shot dispatch | local command and semantic target | bounded CLI result or typed denial | runtime session + policy boundary |

## Integration enumeration

The first slice must prove these real call chains:

1. `NativeEngineConfig` creates a bounded `NativeResourceLoader` and a
   deterministic scheduler.
2. `NativeEngineBackend::new` creates one `NativeEngine` and validates its
   experimental `BackendProfile`.
3. `BackendFactory::native` registers the backend without adding it to
   automatic selection candidates.
4. `BackendFactory::start` permits the native candidate only when the request's
   preferred backend ID is `native-engine`.
5. `BrowserBackendDispatcher::initialize` reaches the engine lifecycle state.
6. `BrowserBackendDispatcher::navigate` reaches resource loading, DOM parsing,
   scheduler commit, history, and revision generation.
7. `BrowserBackendDispatcher::contexts` projects the sole engine context into
   the transport-neutral `BrowsingContext` type.
8. `BrowserBackendDispatcher::evidence` projects the bounded document snapshot
   and rejects screenshot-containing levels.
9. A failed resource or parse path returns before document/history/revision
   mutation.
10. Dispatcher action/effects calls reach the native mutation and revision
    owner; script, storage, prompts, and download calls fail through the
    profile's typed capability gate, while the explicit PNG capture path reads
    the renderer surface without mutation.

## Non-goals for this checkpoint

- network, filesystem navigation, redirects, HTTP semantics, or cookies;
- general CSS parsing/cascade, general raster painting, screenshot semantics,
  or fonts;
  the bounded 009 layout, 010 display-list, 011 software-surface, 013
  clipping, 014 border, and 015 PNG seeds do not imply general layout or
  rendering;
- JavaScript, event loops, timers, storage, workers, Web APIs, or downloads;
- MCP/TUI integration, external browser lifecycle, or platform windows;
- claiming standards compatibility, browser parity, or remote-content safety;
- adding a third crate or a complete-engine dependency.

## Phase 2 and initial Phase 3 slice decomposition

The Phase 2 work is deliberately split into dependency-ordered slices so that
semantic identity is established before mutation and parser state consume it:

| Task | Owns | Depends on | Does not claim |
|---|---|---|---|
| `native-engine-002` | supported role/name projection, bounded attributes, explicit semantic locators, revision-bound references | `native-engine-001` | CSS selectors, layout, hit testing, raw form values |
| `native-engine-003` | click/type/focus mutation for supported controls, checkbox/radio state, revisioned native effects, action/effects backend dispatch | `native-engine-002` | JavaScript, default navigation, coordinate input, full event loop |
| `native-engine-004` | deterministic single-select/option state and semantic option clicks | `native-engine-003` | keyboard navigation, multi-select, submission, network, layout hit testing |
| `native-engine-005` | bounded hidden-state projection, hidden-subtree text exclusion, and pre-mutation visibility gating | `native-engine-004` | CSS selectors/cascade, layout, hit testing, opacity, paint |
| `native-engine-006` | raw-text/RCDATA tokenizer state for script, style, title, and textarea content | `native-engine-005` | HTML5 insertion modes, foreign content, CSS, JavaScript execution |
| `native-engine-007` | bounded compound selectors and display/visibility cascade feeding text/actionability | `native-engine-006` | general CSS, inheritance, layout, hit testing, paint |
| `native-engine-008` | feature-gated `BrowserRuntime::Native`, explicit Rust session construction, local CLI dispatch, and fail-closed runtime validation | `native-engine-007` | remote endpoints, external lifecycle, script/evaluate, MCP/TUI, browser parity |
| `native-engine-009` | bounded integer-pixel normal-flow geometry, Rust layout inspection, deterministic point hit testing, and native `point=x,y` click resolution | `native-engine-008` | general CSS/layout, scrolling, paint, screenshots, stacking contexts, fractional units, browser parity |
| `native-engine-010` | bounded solid-color computed values and revisioned clear/fill/text display-list derivation | `native-engine-009` | rasterization, screenshots, fonts, images, borders, clipping, scrolling, stacking contexts, browser parity |
| `native-engine-011` | bounded logical RGBA software-surface replay with fixed ASCII glyphs and alpha compositing | `native-engine-010` | PNG/screenshots, font shaping, images, borders, clipping, scrolling, stacking contexts, browser parity |
| `native-engine-012` | bounded inherited `color` resolution through DOM ancestors feeding text runs | `native-engine-011` | general CSS inheritance/cascade, inherited layout, fonts, images, screenshots, browser parity |
| `native-engine-013` | bounded `overflow:hidden` ancestor clips on fill/text commands and software replay | `native-engine-012` | scrolling, visible overflow, stacking contexts, borders, transforms, screenshots, browser parity |
| `native-engine-014` | bounded uniform `border:Npx solid <color>` parsing, `BorderRect` display commands, and inside-the-box software replay | `native-engine-013` | padding, box sizing, individual sides, non-solid styles, scrolling, transforms, screenshots, browser parity |
| `native-engine-015` | bounded logical RGBA-to-PNG encoding and real native `CaptureFormat::Png` dispatch | `native-engine-014` | screenshot-containing evidence, JPEG/PDF, physical pixels, viewport/element/full-page modes, fonts, images, browser parity |
| `native-engine-016` | bounded uniform padding/margin, explicit box sizing, outer/content layout rectangles, and content-origin flow/paint | `native-engine-015` | side-specific/negative/percentage/auto values, margin collapsing, positioned/flex/grid layout, scrolling, browser parity |
| `native-engine-017` | bounded root vertical viewport scrolling, content-height/max-offset metadata, coordinate mapping, translated replay/capture, and revisioned scroll action | `native-engine-016` | horizontal/nested/smooth scrolling, scroll anchoring/snap, keyboard scrolling, general overflow/stacking layout, browser parity |
| `native-engine-018` | independently cascaded physical side-specific solid borders, side-aware box insets, display-list paint data, and clipped/scrolled replay | `native-engine-017` | non-solid styles, radius/images/gradients, logical sides/writing modes, corner joins, general CSS, browser parity |
| `native-engine-019` | bounded physical `solid`/`dashed`/`dotted` border styles, typed paint data, deterministic integer patterns, and clipped/scrolled replay | `native-engine-018` | other border styles, radius/images/gradients, standalone style properties, logical sides/writing modes, browser corner metrics, general CSS, browser parity |
| `native-engine-020` | bounded one-to-four-value physical `border-radius`, conservative corner normalization, rounded fill/border replay, and rounded point hit testing | `native-engine-019` | percentages, elliptical radii, corner longhands, rounded descendant clips, transforms, anti-aliasing, browser corner fidelity, general CSS |
| `native-engine-021` | bounded inline-box line placement with preflight width/margin checks and deterministic line-height flow | `native-engine-020` | font shaping/metrics, general word-aware text fragments, CSS whitespace modes, CSS word-breaking variants, baselines, bidi, floats, replaced elements, flex/grid, browser parity |
| `native-engine-022` | bounded positive-pixel `line-height` parsing/cascade, flow minimum line-height, and inline auto-height precedence | `native-engine-021` | `normal`, unitless/relative/percentage values, general inheritance, font metrics, baselines, vertical-align, browser parity |
| `native-engine-023` | bounded direct-text fragments at actual flow origins, collapsed bounded text width, and source-order display-list paint | `native-engine-022` | font metrics/shaping, CSS whitespace modes, general word-aware wrapping, baselines, bidi, cross-node whitespace joining, browser parity |
| `native-engine-024` | bounded word-aware direct-text wrapping with fixed-width fallback for over-wide words | `native-engine-023` | CSS `white-space`/`word-break`/`overflow-wrap`, hyphenation, font metrics/shaping, baselines, bidi, cross-node whitespace joining, browser parity |
| `native-engine-025` | bounded one-to-four-value physical `padding`/`margin` expansion, physical longhands, independent side cascade, and side-aware content/flow geometry | `native-engine-024` | negative/percentage/auto values, logical sides/writing modes, margin collapsing, min/max constraints, positioning, flex/grid, browser parity |
| `native-engine-026` | bounded source-whitespace boundaries across sibling direct text, `display:contents`, and supported inline flow items | `native-engine-025` | CSS `white-space` modes, preserved tabs/newlines, word spacing, Unicode line breaking, bidi, font metrics/shaping, anonymous inline boxes, and browser parity |
| `native-engine-027` | bounded rectangular `overflow:hidden` ancestor clips shared by paint, viewport rectangle projection, and point hit-testing | `native-engine-026` | visible overflow, axis-specific or nested scrolling, rounded descendant clips, stacking contexts, transforms, and browser parity |
| `native-engine-028` | bounded revisioned diagnostics for unsupported selectors, properties, values, and malformed CSS in stylesheet and inline-style sources | `native-engine-027` | general CSS parsing/conformance, raw source echo, stable transport diagnostics, and browser parity |
| `native-engine-029` | complete logical-pixel golden for the bounded native surface and decoded PNG capture path | `native-engine-028` | screenshot compatibility, physical pixels, font/image fidelity, anti-aliasing, color management, stable evidence schema, and browser parity |
| `native-engine-030` | bounded descendant selector chains across the owned DOM ancestry with summed specificity and existing cascade precedence | `native-engine-029` | direct-child/sibling/pseudo/functional/namespace selectors, general CSS conformance, style caching, and browser parity |
| `native-engine-031` | bounded non-scrolling `overflow: clip` through the existing rectangular clip, viewport projection, and point hit-testing path | `native-engine-030` | `visible`/`auto`/`scroll`, axis-specific overflow, nested scrolling, scrollbars, rounded descendant clips, general CSS conformance, and browser parity |
| `native-engine-032` | bounded visible `<br>` hard line breaks through the existing integer inline-flow cursor and fixed line-height floor | `native-engine-031` | `<wbr>`, preserved source newlines, CSS `white-space`, font metrics, Unicode line breaking, bidi, general inline formatting, and browser parity |
| `native-engine-033` | bounded inherited `white-space: pre-line` source newline breaks through the existing hard-break cursor while collapsing other whitespace | `native-engine-032` | `pre`/`pre-wrap`/`break-spaces`/`nowrap`, preserved tabs or arbitrary whitespace, text alignment, font metrics, Unicode line breaking, bidi, and browser parity |
| `native-engine-034` | bounded inherited `white-space: pre` with literal fixed-cell source whitespace, LF/CR/CRLF hard breaks, and no soft wrapping | `native-engine-033` | `pre-wrap`/`break-spaces`/`nowrap`, browser tab stops, wide-line reflow or horizontal scrolling, text alignment, word spacing, Unicode line breaking, bidi, font metrics, and browser parity |
| `native-engine-035` | bounded inherited `white-space: pre-wrap` with literal fixed-cell source whitespace, LF/CR/CRLF hard breaks, and deterministic fixed-cell soft wrapping | `native-engine-034` | `break-spaces`/`nowrap`, browser line-breaking opportunities, tab stops, wide-line scrolling, text alignment/justification, word spacing, Unicode line breaking, bidi, font metrics/shaping, and browser parity |
| `native-engine-036` | bounded standard padded base64 `data:text/html` loading through the existing local resource and dispatcher path with pre-decode and decoded-body bounds | `native-engine-035` | URL-safe or whitespace-tolerant base64, percent-encoded payloads, non-HTML media types, alternate charsets, subresources, network/filesystem loading, cancellation, and browser data-URL parity |
| `native-engine-037` | bounded raw-fragment same-document navigation plus explicit Rust back/forward traversal for local resources, preserving document state and parse-before-commit failure atomicity | `native-engine-036` | anchor scrolling, document snapshots for mutable history state, redirects, HTTP/network history, credentials/cookies, transport-level history operations, and browser navigation parity |
| `native-engine-038` | bounded semantic local anchor activation through the existing click and navigation path for fragment-only and absolute local hrefs, with empty-href click behavior and parse-before-commit failure atomicity | `native-engine-037` | relative URL resolution, remote/network navigation, downloads, target contexts, event propagation/default-action ordering, redirects, and browser navigation parity |
| `native-engine-039` | exact visible local fragment `id` target scrolling plus bounded per-history-entry root-scroll save/restore across same- and different-resource traversal | `native-engine-038` | percent-decoded fragment matching, `name`/text fragments, duplicate-id recovery, smooth/nested/horizontal/keyboard/snap scrolling, sticky layout, mutable DOM snapshots, and browser scrolling parity |
| `native-engine-040` | bounded same-host fixture-relative link resolution through the existing semantic navigation owner, with deterministic URL normalization and existing fragment/history behavior | `native-engine-039` | relative resolution from opaque data/about resources, host-changing references, remote/network navigation, credentials, redirects, downloads, and browser URL-parsing parity |
| `native-engine-041` | bounded UTF-8 percent-decoding of local fragment identifiers before exact visible `id` lookup and existing root-scroll/history restoration | `native-engine-040` | `name`/text fragments, duplicate-id recovery beyond decoded equality, malformed-fragment error policy, general URL decoding, smooth/nested/horizontal/keyboard/snap scrolling, and browser URL-parsing parity |
| `native-engine-042` | bounded exact legacy `<a name>` fragment-target fallback after decoded `id` lookup, with ID precedence and duplicate-safe root-scroll/history behavior | `native-engine-041` | arbitrary `name` attributes, text fragments, duplicate-id recovery, malformed-fragment error policy, general URL decoding, smooth/nested/horizontal/keyboard/snap scrolling, and browser URL/scrolling parity |
| `native-engine-043` | bounded `#:~:text=start[,end]` matching against the first visible non-truncated text run after per-term UTF-8 decoding | `native-engine-042` | prefix/suffix syntax, multiple directives, cross-run ranges, highlights, duplicate-text disambiguation, Unicode normalization, general URL decoding, smooth/nested/horizontal/keyboard/snap scrolling, and browser text-fragment parity |
| `native-engine-044` | bounded exact prefix/suffix text-fragment affixes around a same-run `start[,end]` match | `native-engine-043` | cross-run ranges, multiple directives, highlights, Unicode normalization, general URL decoding, smooth/nested/horizontal/keyboard/snap scrolling, and browser text-fragment parity |
| `native-engine-045` | bounded root horizontal scrolling from measured document overflow width with independent x/y clamping and shared projection | `native-engine-044` | nested scrolling, scrollbars, smooth/keyboard/snap scrolling, axis-specific CSS overflow, scroll anchoring, and browser parity |
| `native-engine-046` | bounded inherited `white-space: nowrap` with collapsed fixed-cell one-line flow and measured root overflow | `native-engine-045` | preserved whitespace modes beyond the existing subset, nested scrolling, scrollbars, Unicode line breaking, font metrics, and browser parity |
| `native-engine-047` | bounded inherited positive-pixel `line-height` through the DOM style walk and existing flow minimum/inline auto-height owners | `native-engine-046` | font-relative, unitless, percentage, CSS-wide keyword, baseline, `vertical-align`, general inheritance, and browser line metrics |
| `native-engine-048` | bounded clip-aware root overflow measurement from the existing document-space overflow intersection | `native-engine-047` | browser overflow propagation, nested scrolling, scrollbars, axis-specific overflow, visible overflow parity, and font metrics |
| `native-engine-049` | bounded independent `overflow-x`/`overflow-y` `hidden`/`clip` rectangles with shorthand/longhand cascade and shared consumers | `native-engine-048` | nested scrolling, scrollbars, mixed visible/auto/scroll used values, rounded descendant clips, and browser overflow parity |
| `native-engine-050` | bounded physical `min-width`/`max-width`/`min-height`/`max-height` constraints through the existing content-box or border-box owner | `native-engine-049` | negative/percentage/auto values, intrinsic sizing, aspect ratio, margin collapsing, positioning, flex/grid, and browser sizing parity |
| `native-engine-051` | bounded local CSS opacity alpha with display-list subtree markers and inside-out transparent-layer software compositing | `native-engine-050` | stacking contexts, transforms, filters, blend modes, animation, compositor parity, and unbounded layer allocation |
| `native-engine-052` | bounded inherited physical `text-align:left|center|right` with complete fixed-cell line-item offsets across text and supported inline boxes | `native-engine-051` | `justify`, logical `start`/`end`, direction/writing modes, vertical alignment, bidi, font metrics, and browser inline-formatting parity |
| `native-engine-053` | bounded functional `rgba(R, G, B, A)` alpha colors for background, border, and text through the existing fixed-point/source-over path | `native-engine-052` | CSS Color 4 syntax, color spaces, interpolation, wide gamut, and color-management parity |
| `native-engine-054` | bounded inherited fixed-cell `text-decoration:none|underline` carried through immutable text commands and clipped software replay | `native-engine-053` | font metrics, decoration propagation, other decoration styles, and browser text-paint parity |
| `native-engine-055` | bounded inherited ASCII `text-transform:none|uppercase|lowercase` applied during fixed-cell text layout and consumed by wrapping, text fragments, paint, and overflow measurement | `native-engine-054` | Unicode case mapping/expansion, locale behavior, `capitalize`/other transforms, font shaping/metrics, and browser text-rendering parity |
| `native-engine-056` | bounded non-negative fixed-pixel `text-indent` applied to the first line of block containers, with one-cell clamping and shared flow consumers | `native-engine-055` | negative/hanging indentation, each-line/hanging keywords, percentages and font-relative units, bidi/logical writing modes, inline-formatting parity, and browser CSS conformance |
| `native-engine-057` | bounded inherited non-negative fixed-pixel `word-spacing` applied to rendered ASCII spaces across collapsed and supported preformatted flow | `native-engine-056` | negative spacing, `letter-spacing`, relative/percentage units, Unicode whitespace and word-boundary policy, browser tab stops, font metrics, bidi, and browser CSS parity |
| `native-engine-058` | bounded inherited non-negative fixed-pixel `letter-spacing` applied after every rendered fixed-cell character in each emitted fragment, composed with word spacing | `native-engine-057` | negative/relative/percentage values, `normal`, pair-boundary and cross-fragment semantics, Unicode shaping/metrics, grapheme clusters, bidi, and browser CSS parity |
| `native-engine-059` | bounded inherited `font-weight: normal|bold|400|700` with deterministic fixed-cell normal/bold raster replay | `native-engine-058` | real font selection/loading/metrics, numeric interpolation, variable fonts, synthetic-bold policy, Unicode shaping, anti-aliasing, and browser text-rendering parity |
| `native-engine-060` | bounded inherited `font-style:normal|italic` with deterministic fixed-cell normal/italic raster replay composed with bold | `native-engine-059` | oblique angles, font selection/loading/metrics, real italic faces, variable fonts, Unicode shaping, anti-aliasing, and browser text-rendering parity |
| `native-engine-061` | bounded inherited `word-break:normal|break-all` with deterministic fixed-cell word-aware or character-boundary wrapping in collapsed flow | `native-engine-060` | `keep-all`, `break-word`, `overflow-wrap`, Unicode/CJK line breaking, grapheme policy, hyphenation, bidi, writing modes, font metrics, and browser CSS parity |
| `native-engine-062` | bounded local `text-overflow:clip|ellipsis` for eligible single-line clipped direct text with fixed-cell ASCII marker presentation | `native-engine-061` | multi-line ellipsis/line-clamp, nested inline formatting, multiple text nodes, visible overflow, vertical/RTL behavior, Unicode ellipsis, grapheme policy, font metrics, shaping, and browser CSS parity |
| `native-engine-063` | bounded inherited `vertical-align:baseline|top|middle|bottom` offsets for fixed-cell inline/inline-block line items | `native-engine-062` | font baselines/metrics, `text-top`/`text-bottom`, sub/super, lengths, percentages, bidi, writing modes, ruby, table-cell/replaced-element alignment, and browser CSS parity |

The DOM remains a single-owner arena. Semantic projections are derived views;
they do not become a second mutable source of truth. The document's current
revision is included in every exported native reference. An action resolves
and validates its locator before any state mutation, then advances the
revision once and records bounded native effects. This gives stale-reference
rejection without exposing arena internals through `browser_backend`.

Phase 2 integration chains added by these slices are:

1. HTML parse -> arena attributes/state -> semantic role/name projection.
2. Current document revision -> native reference generation -> locator
   resolution and stale/detached rejection.
3. Dispatcher action request -> native backend -> locator/actionability check
   -> document mutation -> revision/effect record -> `ActionResult`.
4. Dispatcher effects request -> native engine revision/effect state -> bounded
   `EffectsResult`.
5. Disabled/read-only/ambiguous/unsupported targets fail before mutation.
6. Single-select parsing establishes a deterministic option state; an option
   click updates that group, invalidates its prior reference, and records a
   bounded change effect.
7. Hidden-state derivation excludes hidden text, marks semantic targets, and
   rejects hidden actions before mutation.
8. Raw-text/RCDATA tokenizer state prevents markup-looking script, style,
   title, and textarea content from creating nested semantic elements.
9. Bounded stylesheet and inline declarations produce deterministic
   display/visibility state consumed by visible text and actionability.
10. Feature-gated runtime construction creates the native backend directly from
    `NativeEngineConfig` without contacting an endpoint or entering automatic
    selection.
11. The native CLI path accepts only local URL shapes, forwards semantic
    navigate/click/type/text/observe/targets operations, and rejects unsupported
    flags before startup.
12. Default builds retain the Chromium CLI value set and cannot select native
    through an omitted runtime or a fallback path.
13. Native layout derives visible element rectangles from the current DOM and
    viewport without creating a second mutable owner.
14. Native point clicks reject malformed/out-of-viewport points and resolve
    through the deepest layout hit to an actionable semantic ancestor before
    any mutation.
15. Native display lists are derived from a matching layout revision and emit
    bounded deterministic commands without mutating the document.
16. Native software surfaces replay a bounded display list into logical RGBA
    pixels without mutating page state.
17. Native style resolution inherits only `color` through bounded DOM parent
    links, and display-list text consumes that resolved value.
18. Native fill/text commands carry bounded logical clips derived from matching
    `overflow:hidden` ancestors, and software replay enforces them.
19. Native uniform border declarations produce matching revisioned `BorderRect`
    commands, preserve deterministic fill/border/text order, and replay only
    inside the layout box and its ancestor clips.
20. Native PNG capture encodes the current logical surface through the real
    backend dispatcher, enforces the stable capture-byte limit, preserves the
    revision, and denies unsupported JPEG/PDF formats explicitly.
21. Native box-model derivation exposes outer and content rectangles, applies
    bounded border/padding insets to child/text origins, applies uniform
    margins to normal flow, and preserves deterministic hit-test ownership.
22. Native vertical scroll dispatch derives a bounded root offset, clamps it to
    content height, maps point hits and display replay through that offset, and
    preserves revision/effect behavior for moved/no-op/unsupported deltas.
23. Native side-specific border declarations cascade independently, contribute
    to outer/content geometry, emit bounded display-list paint data, and replay
    with clipping and root-scroll translation.
24. Native solid/dashed/dotted border styles survive physical-side cascade,
    typed display-list projection, deterministic pattern replay, clipping, and
    root-scroll translation while unsupported styles remain ignored.
25. Native bounded physical border radii survive shorthand expansion and
    cascade, concrete-box normalization, layout hit testing, display-list
    projection, rounded fill/border replay, rectangular ancestor clipping, and
    root-scroll translation while unsupported radius forms remain ignored.
26. Native inline element boxes use the same bounded width calculation for
    preflight line-fit decisions and final layout, so adjacent inline boxes
    wrap deterministically before display-list, hit-test, and scroll consumers
    observe their document-space geometry.
27. Native fixed-pixel line-height values are resolved before child flow, so
    direct text and inline boxes share a deterministic minimum line height while
    explicit element heights retain box-model precedence for layout, paint, and
    hit-test consumers.
28. Native direct text is collapsed and fragmented during the same flow pass
    that places inline boxes, so display-list origins and source order match
    layout coordinates while style and ancestor clips remain owned by the
    containing element.
29. Native collapsed direct text keeps a complete word on the current fixed
    line when it fits, drops its separator when it wraps, and splits only a
    word that exceeds the full line width; the resulting fragments remain in
    source order for paint and scroll consumers.
30. Native physical padding and margin shorthands expand deterministically,
    physical longhands cascade per side, and the resulting top/right/bottom/
    left values feed content origins, normal-flow margins, and all existing
    layout/paint/hit/scroll consumers.
31. Native direct-text flow retains bounded leading/trailing whitespace
    boundaries, joins only source-separated inline-flow items, paints consumed
    separators through the existing text path, and drops separators that would
    begin a fresh wrapped line.
32. Native layout derives one bounded rectangular `overflow:hidden` ancestor
    intersection per layout box, and viewport projection plus point hit-testing
    consume that same document-space clip before root-scroll translation.
33. Native stylesheet and inline-style parsing records bounded sanitized
    diagnostics for unsupported selectors, properties, values, and malformed
    rules; document preparation carries the list atomically with navigation and
    the explicit Rust API reports its revision and truncation state.
34. Native direct surface output and decoded bounded PNG bytes match one
    complete checked-in logical-pixel golden while capture preserves revision
    state and does not add screenshot evidence.
35. Native descendant selector chains resolve through the existing DOM parent
    links before style cascade, visibility, layout, and paint consume the
    computed result; unsupported combinators remain diagnosed and ignored.
36. Native `overflow: clip` contributes the same bounded rectangular ancestor
    intersection as `overflow: hidden` before paint, viewport projection, and
    point hit-testing consume it; it never creates nested or implicit scroll.
37. Native visible `<br>` elements advance the containing integer flow cursor by
    one fixed line-height floor, reset the inline origin, and create no
    semantic/layout/paint node; hidden breaks are ignored.
38. Native inherited `white-space: pre-line` turns bounded LF/CR/CRLF source
    boundaries into the same hard-break cursor transition while retaining
    collapsed spaces and the default `white-space: normal` behavior.
39. Native inherited `white-space: pre` retains literal fixed-cell source
    whitespace, turns LF/CR/CRLF into the same hard-break transition, and does
    not soft-wrap preformatted segments without changing semantic ownership.
40. Native inherited `white-space: pre-wrap` retains literal fixed-cell source
    whitespace, turns LF/CR/CRLF into the same hard-break transition, and
    splits source runs only at deterministic fixed-cell soft-wrap capacity
    without synthesizing semantic, layout, or paint nodes.
41. Native standard padded base64 `data:text/html` payloads pass through the
    existing local resource loader, bounded pre-decode/decoded-body checks,
    navigation revision, and real backend dispatcher while retaining opaque
    origin and rejecting invalid, non-UTF-8, percent-encoded, non-HTML, and
    unbounded payloads without echoing their contents.
42. Native raw URL fragments are removed from local resource lookup but retained
    in navigation evidence; same-resource fragment navigation preserves the
    document owner while advancing revision/history and applies the bounded
    visible-id scroll rule, and explicit Rust back/forward traversal reuses or
    reparses bounded local resources atomically at the history cursor
    boundaries.
43. Native semantic local anchor clicks resolve bounded fragment-only,
    fixture-relative-from-fixture-base, or absolute local hrefs through the
    existing navigation owner; successful same-resource activation preserves
    bounded document state, successful cross-resource activation parses before
    commit, and unsupported hrefs fail without mutation.
44. Native same-resource fragment navigation resolves one exact visible local
    `id`, clamps its document-space top to the root viewport, records that
    bounded scroll offset in history, and restores saved offsets at traversal;
    unresolved targets preserve the current offset.
45. Native semantic links resolve non-absolute references only against the
    current registered fixture host, preflight the normalized local resource,
    and retain the existing same-/different-resource commit, fragment-scroll,
    and failure-atomicity paths; unsupported bases and host changes fail
    before mutation.
46. Native local fragment navigation percent-decodes bounded `%HH` byte
    sequences as UTF-8 before exact visible `id` lookup; malformed or invalid-
    UTF-8 sequences are unresolved targets that preserve scroll while the
    accepted URL/revision/history transition remains intact, and literal `+`
    is not converted to a space.
47. Native decoded fragment lookup resolves exact unique `id` before a unique
    legacy `<a name>` fallback, while duplicate IDs/names fail closed and
    non-layout targets preserve the existing scroll offset.
48. Native `#:~:text=start[,end]` navigation decodes terms independently,
    matches the first complete visible layout run in document order, requires a
    same-run ordered end term when present, and preserves the existing
    history/resource transaction for unsupported forms.
49. Native text-fragment prefix/suffix affixes are matched exactly adjacent to
    the start/end terms inside the first complete visible layout run, while
    malformed or cross-run forms preserve the existing scroll fallback.
50. Native root horizontal scroll uses measured document overflow width,
    clamps x and y independently, records both coordinates in history/effects,
    and projects the same offset through viewport rectangles, hit testing,
    display-list replay, and raster capture.
51. Native inherited `white-space: nowrap` collapses supported whitespace while
    preventing soft wrapping, measures the resulting one-line overflow, and
    reaches the existing root horizontal scroll/projection path.
52. Native positive-pixel `line-height` inherits through the bounded DOM style
    walk when a descendant has no valid local declaration, while explicit child
    values and explicit heights retain precedence through the existing flow,
    display, hit-test, and raster owners.
53. Native root `content_width` measures text through the existing bounded
    overflow clip, preventing fully clipped text from creating a false root
    horizontal scroll range while preserving unclipped and partially visible
    output.
54. Native `overflow-x` and `overflow-y` accept only bounded `hidden`/`clip`
    values, cascade independently from the shorthand, and constrain only the
    selected axis through the same paint, viewport, point-hit, and root-overflow
    clip owner.
55. Native bounded min/max width and height constraints apply after the
    existing content-box or border-box inset conversion, preserve minimum-size
    overflow, and feed the same normal-flow, projection, hit-test, paint,
    raster, and root-scroll owners.
56. Native local opacity values quantize to bounded 8-bit alpha and bracket
    reduced-opacity rendered subtrees with display-list markers; software
    replay composites nested groups inside-out under explicit depth and
    aggregate-layer-pixel limits, while layout and point hit testing remain
    unchanged.
57. Native inherited physical text alignment applies deterministic left,
    centered, or right offsets to complete fixed-cell line items, including
    supported inline boxes and direct text, while preserving line breaking,
    dimensions, scrolling, paint, raster, and hit-test ownership.
58. Native inherited ASCII `text-transform:none|uppercase|lowercase` applies
    before fixed-cell whitespace handling and fragmentation so layout,
    wrapping, text-fragment matching, display-list projection, root-overflow
    measurement, and capture share one transformed output, while semantic
    source text remains unchanged.
59. Native non-negative fixed-pixel `text-indent` applies only to the first
    line of block-container flow, reduces that line's fixed-cell capacity,
    resets subsequent lines to the full content width, and shares the resulting
    coordinates with text fragments, display-list projection, hit testing, and
    root-overflow measurement while inline and `display:contents` elements do
    not create an independent indent context.
60. Native inherited non-negative fixed-pixel `word-spacing` adds a bounded
    advance after rendered ASCII spaces before soft wrapping and preformatted
    chunking; the same measured result reaches text fragments, alignment,
    display-list metadata, raster glyph positions, hit testing, and root
    overflow while source text and whitespace ownership remain unchanged.
61. Native inherited non-negative fixed-pixel `letter-spacing` adds a bounded
    advance after every rendered fixed-cell character in each emitted text
    fragment, including the final character; it composes with word spacing on
    ASCII spaces and reaches wrapping, fragments, alignment, display-list
    metadata, raster glyph positions, hit testing, and root overflow without
    changing source text or joining fragment/line/flow boundaries.
62. Native inherited `font-weight: normal|bold|400|700` changes only the
    fixed-cell glyph replay: normal/400 retain existing pixels and bold/700
    dilate set pixels one column to the right. The advance, layout, semantic
    text, clipping, hit testing, overflow, and capture coordinates remain
    unchanged, keeping this presentation rule inside the existing display-list
    and software-raster owners without a font dependency.
63. Native inherited `font-style: normal|italic` changes only the fixed-cell
    glyph replay: normal retains existing pixels and italic shifts each glyph
    row through a bounded deterministic shear. Advances, layout, semantic
    text, clipping, hit testing, overflow, and capture coordinates remain
    unchanged, while bold dilation and other existing text presentation bits
    compose in the same display-list and software-raster owners.
64. Native inherited `word-break: normal|break-all` changes only collapsed
    fixed-cell line breaking: normal keeps the existing word-aware policy while
    break-all permits character-boundary chunks for every word. Collapsed
    separators, transformed text, word/letter spacing, text fragments, flow
    overflow, alignment, display projection, and semantics continue to consume
    the same measured path; `pre`, `pre-wrap`, and `nowrap` retain their
    existing behavior.

The semantic action tradeoff is intentional: it provides a real backend path
for deterministic local fixtures while leaving general geometry to Phase 3.
The 009 seed now gives Phase 3 a bounded geometry owner and executable point
input, but it still cannot claim browser line layout, paint, scrolling, or
visual stacking.

## Tradeoffs and mitigations

| Decision | Benefit | Cost / what we miss | Mitigation |
|---|---|---|---|
| custom small HTML parser | owns the DOM boundary and keeps the default graph unchanged | not HTML5-conformant yet; malformed markup coverage is narrow | explicit Phase 2 conformance work and parser fixtures |
| fixture/data-only loader | deterministic, no SSRF/filesystem risk, fast tests | no real web navigation or network behavior | typed unsupported URL errors and later security workstream |
| in-process single owner | simple revision/history invariants and reproducible tests | no crash isolation or hostile-content safety | keep content local-only; process isolation is a promotion gate |
| no async task callbacks | deterministic scheduler with no hidden sleeps/threads | no script/event-loop realism | typed task kinds and test clock establish the future seam |
| single-select only | useful basic form semantics with a small deterministic state model | no keyboard, multi-select, or submission behavior | reject unsupported variants explicitly and keep values private |
| bounded visibility gate | keeps semantic text/actionability consistent without a CSS dependency | no cascade, layout, opacity, or paint semantics | recognize only explicit hidden signals and document the boundary |
| bounded raw-text/RCDATA modes | prevents fake semantic nodes in embedded text while preserving the small parser | no full HTML5 insertion-mode or foreign-content recovery | keep the mode set explicit and cover unterminated content with fixtures |
| bounded CSS presentation seed | makes stylesheet-driven hiding observable without adding a rendering stack | no general CSS, inheritance, layout, or paint semantics | keep selectors/properties explicit and reject unsupported syntax by omission |
| feature-gated runtime/CLI entry | makes the experiment runnable through the same explicit one-shot contract | native CLI cannot register fixtures, start a browser, or accept remote URLs; feature builds have another compile path | keep default builds unchanged, use the Rust constructor for fixtures, and validate native/default matrices separately |
| bounded normal-flow layout and point hit testing | exercises geometry ownership and input validation without a renderer | no browser line metrics, scrolling, stacking contexts, or fractional CSS | keep rectangles Rust-only, use integer pixels, and reject unsupported dimensions/points explicitly |
| derived display-list seed | establishes a renderer-owned immutable artifact without pixel dependencies | no rasterization, fonts, image decode, clipping, or visual evidence | require a matching layout revision, bound commands, and keep the list Rust-only |
| bounded software surface | makes the display-list contract executable with no graphics dependency | no font fidelity, Unicode shaping, images, screenshots, or physical-pixel guarantees | cap logical pixels, use fixed glyphs, clip writes, and keep the surface Rust-only |
| inherited text color | makes nested text styling observable without broadening the CSS grammar | no general inheritance, user-agent styles, font/color management, or style cache | resolve one property through bounded parent links and keep explicit child declarations authoritative |
| bounded paint clipping | makes descendant overflow behavior explicit without inventing nested scrolling | no nested scroll offsets, visible overflow model, border box-model, transforms, or clip stack cache | carry half-open ancestor intersections on immutable commands and intersect again at replay |
| bounded uniform border paint | adds a useful edge primitive without changing layout ownership | no padding/box sizing, individual sides, non-solid styles, or border geometry | paint an inside-the-box ring as one immutable command and reuse existing clip/source-over bounds |
| bounded PNG capture | makes the current logical renderer artifact consumable through the stable capture operation | no physical-device pixels, screenshot evidence schema, JPEG/PDF, image/font fidelity, or capture modes | encode only the bounded current surface, enforce `MAX_CAPTURE_BYTES`, keep capture read-only, and deny unsupported formats |
| bounded box-model layout | gives border, child flow, text origin, and hit testing one explicit outer/content geometry owner | no general box sizing, logical sides, margin collapsing, fractional metrics, or positioned/flex/grid layout | support only bounded non-negative physical pixel values, expose `content_rect`, and keep unsupported values out of the computed style |
| bounded root viewport scrolling | makes the existing action/layout/paint path observable across a tall local document | no horizontal/nested/smooth scrolling, scroll anchoring, or keyboard behavior | retain document-space boxes, carry one explicit offset, translate only at hit-test/replay boundaries, and clamp all deltas |
| bounded side-specific solid borders | makes common asymmetric card/control edges observable while reusing the existing box model | no non-solid styles, border radius/images/gradients, logical sides, or browser corner joins | cascade four physical sides independently, retain integer geometry, and define deterministic replay precedence |
| bounded dashed/dotted borders | makes common patterned card/control edges observable without adding a CSS painting dependency | no other border styles, radius/images/gradients, standalone style properties, logical sides, anti-aliasing, or browser dash metrics | carry a typed per-side style, anchor bounded integer patterns to document-space geometry, and reuse clip/source-over/root-scroll replay |
| bounded circular border radius | makes common rounded controls/cards and their hit ownership observable without importing a CSS geometry engine | no percentages, elliptical radii, corner longhands, rounded descendant clips, anti-aliasing, or browser corner fidelity | expand one-to-four integer-pixel shorthand values, conservatively normalize to each box, and share the rounded mask between hit testing and software replay |
| bounded inline line placement | prevents inline element boxes from being committed at an overflowing x-coordinate before the cursor is flushed | no font metrics, whitespace/word fragments, baseline/bidi/floats, replaced elements, or flex/grid | preflight the same integer outer width used by final layout, flush whole boxes at line boundaries, and retain deterministic fixed line-height behavior |
| bounded fixed line height | makes fixture line boxes intentionally dense or spacious without importing font metrics | no `normal`, relative/percentage values, general inheritance, baselines, vertical-align, or browser line metrics | parse one positive pixel value, apply it as a local flow floor, and keep explicit height authoritative |
| bounded direct-text flow fragments | prevents mixed direct text and inline descendants from overlapping or being painted at a stale content origin | no font metrics/shaping, CSS whitespace modes, general word breaking, baselines, bidi, cross-node whitespace joining, or browser inline parity | collapse once, fragment from the shared flow cursor, and carry bounded source-order entries into paint |
| bounded word-aware text wrapping | keeps fixture words together when the fixed line can hold them while retaining a bounded fallback for long tokens | no CSS whitespace/word-break/overflow-wrap modes, hyphenation, font metrics/shaping, baselines, bidi, cross-node whitespace joining, or browser inline parity | treat collapsed text as ASCII-space-separated words, place complete words first, and split only over-wide words by fixed character capacity |
| bounded physical box edges | makes asymmetric local fixture geometry observable while preserving one layout owner and no new dependency | no logical writing-mode sides, negative/percentage/auto values, margin collapsing, or general CSS layout | expand bounded physical shorthand values, cascade four sides independently, use saturating integer arithmetic, and feed one document-space geometry to all consumers |
| bounded whitespace boundaries | preserves intentional separators without inventing spaces between separate direct-text nodes | no CSS whitespace modes, preserved tabs/newlines, word spacing, Unicode line breaking, bidi, font metrics/shaping, or cross-owner inline parity | retain only source boundary bits, share one pending separator in the containing flow, paint it through existing fragments, and drop it at line starts |
| bounded overflow hit-test/projection clips | keeps layout visibility and point interaction aligned with existing `overflow:hidden` paint clipping | no visible overflow, nested/axis-specific scrolling, rounded descendant clips, stacking contexts, or transforms | retain one bounded document-space ancestor intersection per layout box and apply it before viewport translation and rounded hit testing |
| bounded unsupported-CSS diagnostics | makes the narrow CSS contract auditable without changing its deterministic fallback behavior | no general CSS parser, conformance location model, raw stylesheet echo, or stable transport capability | retain a fixed diagnostic bound, report sanitized source/category tokens, and replace diagnostics atomically with the prepared document |
| bounded pixel-golden capture | detects full-frame drift in the existing logical renderer and PNG encoder | no physical-pixel, font/image, anti-aliasing, color-management, or screenshot compatibility claim | keep one tiny fixed logical fixture, compare direct and decoded output, and retain capture as a read-only explicit operation |
| bounded descendant selectors | makes ancestor-scoped fixture styles observable through the real DOM ownership chain | no sibling/child/pseudo/functional/namespace selectors, selector caching, general CSS conformance, or browser parity | cap chains at eight compounds, traverse only bounded parent links, sum existing specificity, and preserve explicit unsupported-selector diagnostics |
| bounded `overflow: clip` | removes a common false diagnostic while reusing the existing rectangular clip invariant | no axis-specific overflow, nested scrolling, scrollbars, rounded descendant clips, or public distinction from `hidden` | accept only the single value, map both supported values to one internal clip bit, and preserve bounded diagnostics for `visible`/`auto`/`scroll` |
| bounded hard line breaks | makes author-visible `<br>` structure affect fixture flow without adding a renderer or font engine | no `<wbr>`, preserved source newline, CSS whitespace, font metric, Unicode line-breaking, bidi, or browser inline-formatting claim | move only the existing bounded cursor, emit no synthetic node, and keep hidden/display:none breaks inert |
| bounded `white-space: pre-line` breaks | preserves common source newlines through the proven hard-break transition while retaining a collapsed-space model | no `pre`/`pre-wrap`/`break-spaces`/`nowrap`, tabs, arbitrary whitespace, text alignment, font metrics, Unicode line breaking, bidi, or browser parity | inherit one explicit mode through the existing bounded style walk, normalize CRLF, and reuse the `<br>` flow transition |
| bounded `white-space: pre` | preserves bounded literal source whitespace and line boundaries without inventing soft-wrap or tab-stop policy | no `pre-wrap`/`break-spaces`/`nowrap`, browser tab stops, wide-line reflow or horizontal scrolling, text alignment, word spacing, Unicode line breaking, bidi, font metrics, or browser parity | inherit one explicit mode through the existing style walk, retain fixed-cell text runs, normalize CRLF, and reuse the proven hard-break transition |
| bounded `white-space: pre-wrap` | retains authored whitespace while making bounded preformatted content usable in narrow fixed-cell lines | no `break-spaces`/`nowrap`, browser line-breaking opportunities, tab stops, wide-line scrolling, text alignment/justification, word spacing, Unicode line breaking, bidi, font metrics/shaping, or browser parity | inherit one explicit mode through the existing style walk, split source text at deterministic cell capacity, normalize CRLF, and reuse the proven hard-break transition |
| bounded base64 `data:text/html` loading | makes self-contained local HTML fixtures transportable through the real navigation path without network access | no URL-safe/whitespace-tolerant decoding, percent-encoded base64, alternate charsets, subresources, cancellation, network/filesystem policy, or arbitrary data-URL modes | reject non-HTML metadata, enforce a derived encoded bound before decode and the decoded document limit after decode, require UTF-8, and retain the existing opaque local origin |
| bounded fragment navigation and history traversal | makes local URL/history behavior observable through the existing navigation owner without a second document store or network dependency | no anchor scrolling, mutable document snapshots, redirects, HTTP history, credentials/cookies, or shared transport operation | remove only raw fragments for resource lookup, preserve URL/document state for same-resource entries, parse different resources before moving the cursor, and return explicit boundary no-ops |
| bounded local link activation (038 baseline) | makes existing semantic link clicks reach the proven local navigation/history owner without adding a new transport operation | at the baseline: no relative URL base resolution, remote navigation, download/default-action event loop, target contexts, or network policy | resolve fragment-only and absolute local hrefs, preflight the resource, then commit the existing same-document or parsed navigation path; leave empty href as a click-only action |
| bounded fragment-target scrolling | makes local fragment navigation useful without adding a general scroll engine or retaining full documents | no percent-decoding, name/text fragments, duplicate-id recovery, smooth/nested/horizontal/keyboard/snap scrolling, sticky layout, or browser alignment parity | match one exact raw `id`, use its visible document-space top clamped to the root viewport, and retain one bounded scroll point per history entry |
| bounded fixture-relative link resolution | makes common local relative links usable without opening a network or general origin policy | no relative resolution from opaque data/about resources, host changes, remote schemes, credentials, redirects, downloads, or browser URL parity | resolve with the existing URL parser from a fixture base, enforce the fixture scheme/host, and route the normalized result through the existing loader before mutation |
| bounded percent-decoded fragment targets | makes encoded spaces and UTF-8 local IDs reachable without changing resource lookup or history representation | no arbitrary URL decoding, malformed-fragment error policy, or browser alignment parity | decode only bounded `%HH` bytes as UTF-8, preserve literal `+`, match after decoding, and treat malformed/invalid targets as unresolved while retaining the existing scroll fallback |
| bounded legacy `name` fragment targets | makes common historical `<a name>` anchors reachable without broadening arbitrary attribute semantics | no arbitrary `name` targets, duplicate recovery, or browser URL/scrolling parity | use one exact visible `<a name>` only after no matching `id`, preserve ID precedence, and fail closed on duplicate names |
| bounded text fragments | makes simple text-directed local links useful without a second semantic text tree or highlight renderer | no multiple directives, cross-run ranges, highlighting, Unicode normalization, or browser parity | parse raw term separators before decode, match one visible layout run in document order, and preserve the existing scroll fallback |
| bounded text-fragment affixes | makes common prefix/suffix disambiguation useful without adding a range/highlight tree | no cross-run ranges, highlighting, Unicode normalization, or browser parity | strip only raw affix markers before bounded per-term decoding, require exact adjacency in one visible layout run, and preserve the existing scroll fallback |
| bounded root horizontal scrolling | makes wide deterministic local content reachable through the existing root scroll owner without nested scroll state | no nested scrolling, scrollbars, smooth/keyboard/snap scrolling, axis-specific overflow, or browser parity | derive bounded document width from visible layout output, clamp x/y independently, and reuse the existing projection/history path |
| bounded `white-space: nowrap` | preserves collapsed text on one fixed-cell line so the root horizontal path can reach wide local content | no preserved whitespace, nested scrolling, scrollbars, Unicode line breaking, font metrics, or browser parity | inherit one explicit mode through the existing style walk, disable soft wrapping only for that mode, and measure the resulting text runs |
| bounded inherited fixed `line-height` | keeps nested fixture line boxes consistent with the nearest positive pixel floor without importing font metrics | no relative/unitless/percentage values, CSS-wide keywords, baselines, vertical-align, general inheritance, or browser line metrics | carry only the already-supported positive pixel value through the existing parent walk, preserve explicit child/height precedence, and reuse the current flow minimum |
| bounded clip-aware root overflow | keeps root scrolling consistent with the already shared overflow clip and avoids false scroll ranges from invisible text | no browser overflow propagation, nested scroll state, scrollbars, axis-specific overflow, visible overflow parity, or font metrics | intersect each non-empty non-truncated text run with the existing document-space clip before deriving its right edge; retain box extents and all current projection owners |
| bounded axis-specific overflow | keeps one axis visible while clipping the other through the existing rectangle and independently cascaded CSS declarations | no nested scrolling, scrollbars, mixed visible/auto/scroll used values, rounded descendant clips, or browser overflow parity | carry two bounded clip bits, map the selected axis to a finite document-space range, and reuse paint/projection/hit-test/root-overflow consumers |
| bounded min/max dimensions | makes common minimum and maximum box constraints observable without replacing the integer box-model owner | no negative/percentage/auto values, intrinsic sizing, aspect ratio, margin collapsing, positioning, flex/grid, or browser sizing parity | convert bounds through existing insets, apply lower/upper constraints before available-width clamping, and preserve deterministic outer/content rectangles |
| bounded opacity groups | makes local translucent subtrees compose as one paint group without changing the layout owner | off-screen layers add memory and a second pixel pass; no stacking contexts, transforms, filters, animation, or browser compositor parity | parse bounded fixed-point alpha, bracket only reduced-opacity rendered subtrees, cap group depth and aggregate layer pixels, and keep opacity out of layout/hit visibility |
| bounded text alignment | keeps direct text and supported inline boxes visually and interactively together within a line | no justification, logical-direction, writing-mode, vertical-alignment, bidi, font-metric, or browser inline-formatting parity | track bounded line-item artifact ranges and apply deterministic fixed-cell left/center/right offsets at line flush without changing line breaks or dimensions |
| bounded functional alpha colors | makes common translucent background, border, and text fixtures expressible through the existing paint path | no CSS Color 4 syntax, channel percentages, modern space-separated functions, wide-gamut colors, interpolation, or color-management parity | parse decimal integer channels plus the existing fixed-point alpha grammar, then reuse immutable `NativeColor` and integer source-over replay |
| bounded fixed-cell text decoration | makes a deterministic one-pixel underline available across supported text fragments | no font metrics, descender-aware placement, decoration propagation parity, styles, colors, thickness, offsets, overline, line-through, blink, or browser parity | inherit one bounded `none`/`underline` value, carry it in immutable text commands, and draw a clipped alpha-aware line at the fixed glyph baseline offset |
| bounded inherited text transform | makes common ASCII case presentation available across the existing fixed-cell text-flow path | no Unicode case mapping or expansion, locale-sensitive casing, `capitalize`/other transforms, font shaping/metrics, or browser text-rendering parity | inherit one bounded `none`/`uppercase`/`lowercase` value, transform ASCII letters during layout, preserve source semantic text, and keep fixed-cell width invariant |
| bounded first-line text indent | makes common block first-line indentation observable without a second formatting context | no negative/hanging indentation, keyword/percentage/font-relative units, bidi/logical writing modes, or browser CSS parity | keep one non-negative fixed-pixel value in computed style, clamp it to leave one fixed cell, and switch the existing flow cursor to full width after the first line |
| bounded inherited word spacing | makes common fixed-pixel separator spacing observable across the existing text-flow and paint owners | no negative spacing, `letter-spacing`, relative/percentage units, Unicode whitespace or word-boundary policy, browser tab stops, font metrics, bidi, or browser parity | inherit one bounded non-negative pixel value, add it only after rendered ASCII spaces, and carry the measured advance through wrapping, fragments, alignment, display, raster, hit testing, and overflow |
| bounded inherited letter spacing | makes a bounded per-character fixed-cell advance observable across the existing text-flow and paint owners | no negative/relative/percentage values, `normal`, pair-boundary or cross-fragment semantics, Unicode shaping/metrics, grapheme clusters, bidi, or browser parity | inherit one bounded non-negative pixel value, add it after every rendered character in each emitted fragment, compose it with word spacing, and carry the measured advance through wrapping, fragments, alignment, display, raster, hit testing, and overflow |
| bounded inherited font weight | makes normal and bold fixed-cell text presentation observable without changing geometry | real font selection/loading/metrics, numeric interpolation, variable fonts, synthetic-bold policy, Unicode shaping, anti-aliasing, and browser text-rendering parity | inherit one normalizable two-state value, keep fixed-cell advances unchanged, carry it on immutable text commands, and dilate bold glyph pixels through the existing clipped software replay |
| bounded inherited font style | makes normal and italic fixed-cell text presentation observable without changing geometry | oblique angles, real italic faces, font selection/loading/metrics, variable fonts, Unicode shaping, anti-aliasing, and browser text-rendering parity | inherit one normal/italic value, keep fixed-cell advances unchanged, carry it on immutable text commands, and apply a bounded row-dependent shear through the existing clipped software replay |
| bounded inherited word break | makes explicit character-boundary wrapping available for collapsed fixed-cell words | `keep-all`, `break-word`, `overflow-wrap`, Unicode/CJK line breaking, grapheme policy, hyphenation, bidi, writing modes, font metrics, and browser parity | inherit one normal/`break-all` value, retain the current separator policy, and reuse fixed-cell capacity to split every collapsed word without changing semantic source text |
| bounded text overflow | makes clipped single-line fixture text visibly indicate an omitted suffix without changing semantic source text | multi-line truncation, line-clamp, nested inline formatting, multiple text nodes, visible overflow, RTL/vertical writing, Unicode ellipsis, grapheme policy, font metrics, shaping, and browser parity | resolve local `clip`/`ellipsis`, require a rendered nowrap block with one direct text child and horizontal clipping, then reuse fixed-cell prefix measurement and immutable text commands for a bounded ASCII marker |
| bounded vertical alignment | makes fixed-cell inline items visibly align within the existing line box without adding a typographic baseline engine | font ascent/descent, real baselines, `text-top`/`text-bottom`, sub/super, lengths, percentages, bidi, writing modes, ruby, table-cell/replaced-element alignment, and browser parity | inherit four bounded keywords, keep the current line-box height and horizontal flow, and shift each recorded inline item's box/text artifact range by a clamped top/middle/bottom offset during line flush |
| no new dependencies | preserves build time and supply-chain surface | parser/rendering work is slower to build ourselves | keep boundaries explicit; evaluate focused libraries only per issue rules |

## Delivery evidence

The task file for each slice owns its touched paths and verification commands;
`docs/plan/tasks/native-engine-063.md` is the active design checkpoint;
`docs/plan/tasks/native-engine-062.md` is the latest completed checkpoint;
`docs/plan/tasks/native-engine-061.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-060.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-059.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-058.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-057.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-056.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-055.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-054.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-053.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-052.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-051.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-050.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-049.md` is the earlier completed checkpoint;
`docs/plan/tasks/native-engine-048.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-047.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-046.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-045.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-044.md` is an earlier completed checkpoint;
`docs/plan/tasks/native-engine-043.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-042.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-041.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-040.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-039.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-038.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-037.md` is the preceding completed checkpoint;
`docs/plan/tasks/native-engine-036.md` is the preceding completed checkpoint,
`docs/plan/tasks/native-engine-035.md` is the preceding completed checkpoint,
`docs/plan/tasks/native-engine-034.md` is an earlier completed checkpoint,
`docs/plan/tasks/native-engine-032.md` is an earlier completed checkpoint,
`docs/plan/tasks/native-engine-031.md` is the earlier completed checkpoint,
and `docs/plan/tasks/native-engine-030.md` is the earlier selector checkpoint.
A checkpoint is complete only when
the native feature tests pass, strict lint
passes for the touched code, and the diff confirms no unrelated browser/TUI/
release behavior changed.
