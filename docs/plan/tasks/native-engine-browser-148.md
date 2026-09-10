---
id: native-engine-browser-148
scope: glass-browser/native-engine/window-open-named-contexts
status: completed
depends-on: [native-engine-browser-147]
---

# Native engine browser slice 148: script-created browsing contexts

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Make script-created page targets a native browser-owner behavior. A page must
be able to call `window.open()` without CDP, the content worker must transfer
the request safely, and the native target registry must create or reuse the
correct browsing context while keeping the opener selected.

## Contract

- the native JavaScript host exposes `window.open(url, target)` and resolves a
  relative URL against the opener's current document before emitting a typed
  request;
- omitted and empty URLs create `about:blank` targets, `_blank` creates a new
  independent target, and `_self`, `_parent`, `_top`, and `_unfencedTop` use
  the existing current-context navigation path;
- non-reserved target names create a named browsing context on first use and
  navigate/reuse that same target on later calls instead of creating a second
  target;
- the returned JavaScript value is a bounded WindowProxy-shaped object with
  `name`, `closed`, and `close()` state, while native target ownership remains
  with the parent backend;
- local/data/fixture pages and HTTP(S) pages use the same request and target
  ownership path; HTTP(S) requests cross the version-5 content-worker IPC;
- popup requests emitted during page load, direct script evaluation, event
  handlers, lifecycle dispatch, fetch continuation, or child target loading
  are transferred without being mistaken for DOM mutations;
- target creation is initialized before publication, inherits the native
  configuration and opener relationship, remains parked/non-selected, and
  nested popup cascades are bounded and processed in order;
- named-target navigation replaces the named target's document and resets its
  frame registry without changing the opener's active selection;
- malformed popup URL/name data, target-name control bytes, target limits, and
  excessive popup cascades fail closed without publishing a half-created
  target;
- existing anchor `_blank`, download precedence, popup-click witnesses,
  target listing, target selection, storage, history, and frame ownership
  continue to use the same native target pool.

## Tradeoffs

Popup creation is represented as a typed intent rather than exposing a native
engine pointer to JavaScript. This keeps the QuickJS realm and content worker
isolated from target-registry locks and lets the parent serialize named-target
reuse, but the returned WindowProxy is intentionally a capability-shaped
handle until cross-context scripting and `postMessage` are implemented.

Named targets are tracked privately on the target owner because the stable
Glass `PageTargetInfo` contract does not yet expose a browsing-context name.
This preserves the public schema and lets existing target IDs remain stable;
selecting a named target still carries its opener relationship and its parked
document state.

Popup cascades are processed iteratively with a bounded budget. This permits
ordinary page-load scripts that open another page while preventing an
unbounded target storm from consuming the target pool or the content-worker
startup budget.

## Implementation surface

- `browser/native_engine/javascript.rs`: typed `window.open` command, popup
  queue, reserved-target navigation, and bounded WindowProxy-shaped result;
- `browser/native_engine/dom.rs`: popup commands remain host effects rather
  than document mutations;
- `browser/native_engine/content_process.rs`: version-5 popup transfer,
  response decoding, and runtime-state synchronization for load, script,
  lifecycle, and event paths;
- `browser/native_engine/engine.rs`: parent-owned popup queue draining from
  local and worker-backed execution;
- `browser/native_backend.rs`: named-target metadata, initialized target
  materialization, reuse navigation, nested cascade handling, and selection
  preservation;
- `tests/native_engine.rs`: local page-load creation/reuse and HTTP content-
  worker creation/reuse witnesses.

## Verification

```text
cargo fmt --all
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine native_window_open -- --nocapture
cargo test --quiet -p glass-browser --features native-engine --test native_engine http_window_open -- --nocapture
```

The local and HTTP content-worker witnesses pass. The full native integration
suite, strict affected-package lint, workspace gates, documentation
validators, package/release gates, and remote CI remain required before issue
#40 production promotion and closure.
