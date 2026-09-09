---
id: native-engine-browser-017
scope: glass-browser/native-engine/javascript-host-view
status: done
depends-on: [native-engine-browser-016]
---

# BE-04b: bounded JavaScript host view

## Objective

Expose the current native document to the already-bounded JavaScript realm in
both local and child-owned execution paths. This slice establishes one
versioned, read-only host projection and promise completion behavior without
pretending that a JavaScript VM is a live DOM, event loop, or Web IDL
implementation.

## Contract

- Every script evaluation refreshes a host projection from the current native
  document before running page code. Local documents and external HTTP(S)
  documents receive the same projection; external page code still executes
  only in the sandboxed content process.
- `window` points at the global object. The projection exposes bounded
  `document.title`, `document.body`, `document.documentElement`,
  `document.readyState`, `getElementById`, `querySelector`,
  `querySelectorAll`, `getElementsByTagName`, and `getElementsByClassName`.
  The selector subset is explicit: ID, class, tag, and `*` where supported by
  the corresponding finder.
- Projected elements expose stable-in-one-evaluation `nodeIndex`, uppercase
  `tagName`, `id`, `className`, bounded `textContent`/`innerText`, form state,
  `hidden`/`disabled`, `getAttribute`, and `hasAttribute`. These are snapshots;
  they are not live Web IDL objects and do not retain identity across the next
  evaluation or document revision.
- `location.href` retains the committed URL, while `location.origin` comes
  from Glass's normalized origin owner (`"null"` for opaque local origins).
  `innerWidth` and `innerHeight` come from the validated native viewport.
  `navigator.userAgent` is the bounded `GlassNative` marker and `console` is a
  no-op diagnostic surface so ordinary inspection code does not write to the
  worker protocol.
- Element mutation entry points intentionally fail with `TypeError` until
  DOM mutation/event integration owns command collection, validation, event
  dispatch, revision advancement, and rollback. This prevents a JavaScript
  object mutation from silently diverging from the Rust document owner.
- Synchronous scripts preserve their direct completion value. Scripts that
  require top-level `await` use the QuickJS promise evaluator and complete
  bounded microtasks before result conversion. The public result remains the
  existing bounded JSON value; promise wrapper details do not cross the
  backend boundary.
- The existing 16 KiB source, 64 KiB JSON result, 32 MiB memory, 1 MiB stack,
  and five-second interrupt limits remain in force. Script exceptions do not
  poison a healthy child; transport, timeout, and malformed-transfer failures
  retain typed worker recovery behavior.

## Deliberate boundary and tradeoffs

- A snapshot is fast, deterministic, and easy to bound, but it misses live
  identity, prototypes, accessors, `NodeList`/`HTMLCollection` semantics,
  mutation observers, event listeners, and computed Web IDL conversions.
- The current parser does not synthesize missing HTML5 `html`/`head`/`body`
  elements for fragment-shaped input, so `document.body` or
  `document.documentElement` may be `null`. HTML5 tree-builder conformance is
  a separate parser gate.
- Promise completion handles synchronous QuickJS jobs and top-level promise
  results, but timers, host-backed asynchronous futures, network callbacks,
  module loading, and a browser event loop remain absent.
- `navigator`, `console`, and the element surface are intentionally minimal;
  they are compatibility footholds, not a claim of standards completeness.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` — 287/287 passed.
- Coverage includes local and child-owned host lookup, opaque and tuple
  origins, persistent globals, synchronous results, top-level `await`, and
  existing native navigation/IPC regressions.
- `git diff --check`

The next gate is live DOM mutation and event integration: JavaScript must
produce validated owner commands, apply them transactionally to the native
document, advance revisions exactly once, and expose effects without
bypassing the existing semantic/action policy. Fetch/XHR, timers, modules,
page-script loading, remaining resource classes, and browser parity remain
open.
