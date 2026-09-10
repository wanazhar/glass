---
id: native-engine-browser-125
scope: glass-browser/native-engine/location-navigation
status: done
depends-on: [native-engine-browser-124]
---

# BE-39: bounded live location navigation

## Objective

Give the native JavaScript realm a live `window.location` owner that can drive
bounded navigation through the existing Rust/content-process navigation path.

## Contract

- `location` exposes bounded live `href`, `origin`, `protocol`, `username`,
  `password`, `host`, `hostname`, `port`, `pathname`, `search`, and `hash`
  projections plus `assign()`, `replace()`, `reload()`, and `toString()`.
- The outer location object is frozen, while its private URL owner remains
  mutable. `href` and component setters resolve through the existing bounded
  URL owner and emit one typed navigation command after successful parsing.
- `assign()`, `href`, component setters, and `reload()` use push history;
  `replace()` uses replace-current-entry history without truncating the
  forward list before a later successful navigation.
- Relative location references are resolved by the Rust navigation owner;
  local fixtures, HTTP(S) resources, same-document fragments, lifecycle, and
  content-worker transfer retain their existing validation and commit paths.
- Content-process navigation metadata is validated as either a DOM-targeted
  link/form handoff or a target-free location handoff. A location handoff must
  use node index zero, no submitter, and cannot carry replacement metadata as a
  link/form operation.
- Location commands emitted during page-load evaluation or ownerless lifecycle
  callbacks fail with a typed owner error. They are not silently discarded;
  queued/re-entrant event navigation remains a future event-loop gate.

## Ownership and sequence

```text
JavaScript location owner
        -> Navigate command
        -> local or content-process validation
        -> NativeNavigationRequest
        -> bounded loader
        -> same/full-document commit
        -> push or replace history entry
```

The Rust engine remains authoritative for URL resolution, resource policy,
origin, lifecycle, worker recovery, and history mutation. The JavaScript
location object only projects the current URL and requests a transition.

## Deliberate boundary and tradeoffs

This slice makes ordinary post-commit script-driven navigation observable and
usable without claiming the full HTML navigation algorithm. It deliberately
does not add queued navigation from page-load/lifecycle callbacks, nested
browsing contexts, download/navigation targets, complete WHATWG URL parsing,
or browser-wide Location/Web IDL descriptors. Failing closed at ownerless
event boundaries preserves transaction ownership and makes the unsupported
re-entrant path diagnosable.

## Paths

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/history.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

The slice was validated through the local fixture and process-backed native
integration paths. No remote CI, push, release, tag, or registry-publication
claim is made by this local checkpoint.

- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_location_owns_navigation_and_history_replacement -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_location_navigation_crosses_worker_boundary -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 393 passed in 68.78s
- `cargo fmt --all -- --check` and `git diff --check` — passed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine -- -D warnings` — blocked by 34 pre-existing workspace lint diagnostics outside the location/history changes; no new diagnostic identified in the touched path
- `python3 scripts/check-documentation-coverage.py` — 775 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  775 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=949; current-claim failures=0
- Strict-lint, package, cross-platform, and browser-profile gates remain
  issue-level gates.
