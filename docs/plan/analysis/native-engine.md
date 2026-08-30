# Native engine Phase 1/Phase 2 and initial Phase 3 analysis

Status: Active implementation analysis for issue #40; Phase 0/1 and the first
Phase 2 semantic/action/form-control/parser slices and the initial Phase 3
presentation slice are committed locally.

Issue [#40](https://github.com/wanazhar/glass/issues/40) is the authority. The
current delivery is a Phase 0/Phase 1 kernel plus bounded Phase 2 semantic
DOM/interaction slices, not an attempt to implement a complete browser in one
change.

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
| `native_engine::css` | bounded selector/rule parsing and display/visibility presentation state | style text, inline style, native element attributes | deterministic per-element presentation flags | native DOM element surface |
| `native_engine::dom` | arena DOM, semantic projection, and bounded control/form mutation | HTML source, locators, and limits | generational nodes/document evidence/effects | native limits |
| `native_engine::interaction` | action/effect types and bounded effect records | semantic action and event kind | revisioned interaction metadata | native DOM IDs |
| `native_engine::engine` | sole mutable page-state coordinator | lifecycle/navigation/action requests | snapshots/context/history/effects | all engine modules |
| `browser::native_backend` | semantic adapter/profile | backend requests | typed backend responses/errors | engine + `browser_backend` |

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
    owner; script, capture, storage, prompts, and download calls fail through
    the profile's typed capability gate.

## Non-goals for this checkpoint

- network, filesystem navigation, redirects, HTTP semantics, or cookies;
- general CSS parsing/cascade, layout, hit testing, painting, screenshots, or fonts;
- JavaScript, event loops, timers, storage, workers, Web APIs, or downloads;
- CLI runtime selection, TUI integration, MCP integration, or platform windows;
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

The semantic action tradeoff is intentional: it provides a real backend path
for deterministic local fixtures while leaving geometry ownership to Phase 3.
The engine can therefore test identity, actionability, mutation, and revision
contracts now, but cannot claim that a target is visually hit-testable or that
clicking a link performs browser navigation.

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
| profile exposes six capabilities | accurate discovery and fail-closed operations | no user-facing native CLI path yet; semantic actions are intentionally narrow | public Rust factory first; CLI/runtime integration is a later task |
| no new dependencies | preserves build time and supply-chain surface | parser/rendering work is slower to build ourselves | keep boundaries explicit; evaluate focused libraries only per issue rules |

## Delivery evidence

The task file for each slice owns its touched paths and verification commands;
the current parser/interaction/presentation slice is `docs/plan/tasks/native-engine-007.md`. A
checkpoint is complete only when the default feature set remains green, the
native feature tests pass, strict lint passes for the touched code, and the
diff confirms no unrelated browser/TUI/release behavior changed.
