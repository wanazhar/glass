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
| `native_engine::config` | public startup configuration and hard limits | URLs, viewport, fixtures, limits | validated `NativeEngineConfig` | `url`, typed native error |
| `native_engine::lifecycle` | lifecycle state | transitions | `New`, `Running`, `Closed` | none |
| `native_engine::scheduler` | logical clock and bounded ordered tasks | task kind, delay | deterministic task IDs/order | native limits |
| `native_engine::history` | current local history | committed URL/revision | bounded entries/current index | native limits |
| `native_engine::origin` | Phase 1 origin placeholder | loaded URL | opaque origin | none |
| `native_engine::resource_loader` | fixture/data/about resource boundary | validated URL | bounded local HTML resource | `url`, config fixtures |
| `native_engine::css` | bounded selector/rule parsing, display/visibility presentation, inherited color, pixel dimensions, fixed pixel line-height, physical solid/dashed/dotted borders, circular border radii, and physical padding/margin edges | style text, inline style, native element attributes, ancestor styles | deterministic computed presentation values | native DOM element surface |
| `native_engine::layout` | viewport-bounded block/inline normal-flow geometry, bounded outer/content box model, side-specific border insets, rounded-box metadata, preflight inline line placement, fixed line-height floors, direct-text fragments, whitespace-boundary flow, source-order paint entries, root scroll projection, and rounded point hit testing | DOM, computed presentation, viewport, scroll offset | document-space layout boxes/text fragments, paint order, scroll metadata, and deterministic hit target | native DOM + CSS presentation |
| `native_engine::paint` | revisioned clear/fill/text-fragment/physical-border display-list derivation, bounded rounded paint masks, source-order entries, ancestor clips, and scroll metadata | current layout, bounded computed colors/text/borders/radii, and overflow presentation | immutable document-space display-list commands | native DOM + layout |
| `native_engine::raster` | bounded logical RGBA surface replay for fills, text, rounded solid/dashed/dotted borders, PNG encoding, and viewport translation | immutable display-list commands and scroll offset | immutable software surface or bounded PNG bytes | native display list + existing `png` dependency |
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

The semantic action tradeoff is intentional: it provides a real backend path
for deterministic local fixtures while leaving general geometry to Phase 3.
The 009 seed now gives Phase 3 a bounded geometry owner and executable point
input, but it still cannot claim browser line layout, paint, scrolling, visual
stacking, or that clicking a link performs browser navigation.

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
| no new dependencies | preserves build time and supply-chain surface | parser/rendering work is slower to build ourselves | keep boundaries explicit; evaluate focused libraries only per issue rules |

## Delivery evidence

The task file for each slice owns its touched paths and verification commands;
`docs/plan/tasks/native-engine-033.md` is the latest completed checkpoint,
`docs/plan/tasks/native-engine-032.md` is the preceding completed checkpoint,
`docs/plan/tasks/native-engine-031.md` is the earlier completed checkpoint,
and `docs/plan/tasks/native-engine-030.md` is the earlier selector checkpoint.
A checkpoint is complete only when
the native feature tests pass, strict lint
passes for the touched code, and the diff confirms no unrelated browser/TUI/
release behavior changed.
