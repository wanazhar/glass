# Native engine Phase 1 analysis

Status: Active implementation analysis for issue #40.

Issue [#40](https://github.com/wanazhar/glass/issues/40) is the authority. The
first delivery is a Phase 0/Phase 1 checkpoint, not an attempt to implement a
complete browser in one change.

## Baseline and constraints

The current checkout has exactly two installable crates. The native engine
must stay inside `glass-browser`, remain default-off, and add no dependency in
this checkpoint. Chromium/CDP remains the production path; native selection is
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
default path only because it adds the Phase 1 module set; it adds no external
dependency. A future performance task should record clean builds, a real edit
touching the native module, and target-directory growth separately.

## Module decomposition

| Module | Owns | Inputs | Outputs | Dependencies |
|---|---|---|---|---|
| `native_engine::config` | public startup configuration and hard limits | URLs, viewport, fixtures, limits | validated `NativeEngineConfig` | `url`, typed native error |
| `native_engine::lifecycle` | lifecycle state | transitions | `New`, `Running`, `Closed` | none |
| `native_engine::scheduler` | logical clock and bounded ordered tasks | task kind, delay | deterministic task IDs/order | native limits |
| `native_engine::history` | current local history | committed URL/revision | bounded entries/current index | native limits |
| `native_engine::origin` | Phase 1 origin placeholder | loaded URL | opaque origin | none |
| `native_engine::resource_loader` | fixture/data/about resource boundary | validated URL | bounded local HTML resource | `url`, config fixtures |
| `native_engine::dom` | arena DOM and text/title projection | HTML source and limits | generational nodes/document evidence | native limits |
| `native_engine::engine` | sole mutable page-state coordinator | lifecycle/navigation requests | snapshots/context/history | all engine modules |
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
10. Dispatcher calls for action, effects, script, capture, storage, prompts,
    and downloads fail through the profile's typed capability gate.

## Non-goals for this checkpoint

- network, filesystem navigation, redirects, HTTP semantics, or cookies;
- CSS parsing, cascade, layout, hit testing, painting, screenshots, or fonts;
- JavaScript, event loops, timers, storage, workers, Web APIs, or downloads;
- CLI runtime selection, TUI integration, MCP integration, or platform windows;
- claiming standards compatibility, browser parity, or remote-content safety;
- adding a third crate or a complete-engine dependency.

## Tradeoffs and mitigations

| Decision | Benefit | Cost / what we miss | Mitigation |
|---|---|---|---|
| custom small HTML parser | owns the DOM boundary and keeps the default graph unchanged | not HTML5-conformant yet; malformed markup coverage is narrow | explicit Phase 2 conformance work and parser fixtures |
| fixture/data-only loader | deterministic, no SSRF/filesystem risk, fast tests | no real web navigation or network behavior | typed unsupported URL errors and later security workstream |
| in-process single owner | simple revision/history invariants and reproducible tests | no crash isolation or hostile-content safety | keep content local-only; process isolation is a promotion gate |
| no async task callbacks | deterministic scheduler with no hidden sleeps/threads | no script/event-loop realism | typed task kinds and test clock establish the future seam |
| profile exposes only four capabilities | accurate discovery and fail-closed operations | no user-facing native CLI path yet | public Rust factory first; CLI/runtime integration is a later task |
| no new dependencies | preserves build time and supply-chain surface | parser/rendering work is slower to build ourselves | keep boundaries explicit; evaluate focused libraries only per issue rules |

## Delivery evidence

The task file `docs/plan/tasks/native-engine-001.md` owns the touched paths and
verification commands. A checkpoint is complete only when the default feature
set remains green, the native feature tests pass, strict lint passes for the
touched code, and the diff confirms no unrelated browser/TUI/release behavior
changed.
