---
id: native-engine-browser-016
scope: glass-browser/native-engine/javascript-realm
status: done
depends-on: [native-engine-browser-015]
---

# BE-04a: bounded JavaScript realm

## Objective

Replace the native backend's script capability denial with a real, persistent
ECMAScript realm while keeping the browser host surface explicit. This slice
adds execution semantics without pretending that a JavaScript VM supplies the
DOM, Fetch, timers, or other Web APIs by itself.

## Contract

- `BrowserCapability::Script` is available for the native backend and dispatches
  `ScriptRequest` through the existing semantic backend contract.
- Local `about:blank`, data, and fixture documents evaluate in an owner-side
  QuickJS realm. HTTP(S) documents evaluate in the already sandboxed,
  child-owned content process. The parent never evaluates external page code.
- The realm persists across script calls and same-document fragment navigation.
  A successful full navigation replaces it, so page globals do not leak across
  documents. A child load also clears its realm before publishing the new page.
- Results are converted through `JSON.stringify` into bounded JSON. `undefined`
  and values without a JSON representation become the explicit JSON `null`
  result; cyclic or otherwise non-serializable values fail as typed script
  errors. Script source is capped at the existing 16 KiB semantic request
  limit, results at 64 KiB, runtime memory at 32 MiB, stack at 1 MiB, and
  execution at five seconds through a QuickJS interrupt deadline.
- JavaScript exceptions remain ordinary script failures and do not poison a
  healthy content process. IPC failures, malformed transfers, and execution
  deadlines retain the existing typed worker-failure and recovery behavior.
- The dependency is optional and only enabled by the default-off
  `native-engine` feature. `rquickjs` supplies ECMAScript; Glass still owns
  every browser host binding.

## Deliberate boundary and tradeoffs

- This is real ECMAScript execution, not browser compatibility. `window`,
  `document`, DOM objects, Web IDL conversions, `fetch`, XHR, timers, modules,
  event dispatch, storage, workers, and page-script loading remain subsequent
  gates.
- QuickJS is compact and embeddable, but the optional native feature now has a
  C/FFI build and uses the crate's `parallel` support so the backend's stable
  `Send + Sync` contract remains valid. Default Glass builds do not compile
  this dependency.
- JSON serialization keeps the current transport bounded and deterministic but
  loses identity/prototype/accessor semantics. A later Web IDL result model must
  be versioned rather than silently widening this JSON contract.
- The owner-side local realm is not a hostile-content boundary. External pages
  remain process-backed; local caller-provided fixtures/data are trusted test
  inputs for this slice.

## Paths

- `crates/glass-browser/Cargo.toml`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test -p glass-browser --features native-engine --test native_engine -- --nocapture` — 286/287 passed before replacing a stale external-network assertion; the corrected focused regression and both local/child JavaScript tests pass.
- `git diff --check`

The next gate is the host/Web IDL bridge: page-owned `window`/`document`
objects, DOM mutation/event integration, Promise driving, and the Fetch
request/response model. This task does not claim script-loaded resources,
ordinary web-app compatibility, or CDP replacement.
