---
id: native-engine-browser-159
scope: glass-browser/native-engine/cross-origin-window-security
status: completed
depends-on: [native-engine-browser-158]
---

# Native engine browser slice 159: cross-origin Window security

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Make projected frame windows honor the caller-relative same-origin boundary.
The frame bridge already withheld cross-origin documents, but it still exposed
parent metadata and generic JavaScript errors inconsistently. This slice makes
the boundary observable and recoverable in both local and sandboxed HTTP(S)
realms.

## Contract

- cross-origin `contentDocument` remains `null`;
- cross-origin `WindowProxy.document` throws a `DOMException` named
  `SecurityError` with legacy code `18`;
- cross-origin access to the bounded sensitive Window properties
  (`history`, storage, `indexedDB`, `navigator`, `performance`, `screen`, and
  `crypto`) throws the same typed error;
- cross-origin frame windows expose only the safe bounded surface already
  owned by Glass: URL/location navigation, `name`, `closed`, `close`,
  `postMessage`, `parent`, `top`, `frames`, and bounded indexed access;
- a caller cannot recover a cross-origin embedding `frameElement`; a selected
  cross-origin child reports `window.frameElement === null`;
- same-origin projected frames retain the existing document, element,
  collection, parent/top, `frameElement`, message, navigation, and identity
  behavior;
- origin state refreshes when a cached WindowProxy target navigates, so a
  same-origin target that crosses an origin boundary is not left privileged;
- the parent-owned frame registry and target/storage ownership contracts are
  unchanged, and no engine pointers or raw page data cross the snapshot wire.

## Tradeoffs

The bridge exposes a deliberately finite cross-origin surface rather than a
general JavaScript membrane. This keeps the snapshot model bounded and avoids
inventing access to APIs that are not implemented in the native realm. The
native `DOMException` constructor is a small compatibility projection rather
than a claim of complete Web IDL descriptor parity. URL reads and controlled
navigation remain available because they are the browser's safe cross-origin
Window operations; document, storage, and execution state remain isolated.

## Implementation surface

- `crates/glass-browser/src/browser/native_engine/javascript.rs`: bounded
  `DOMException`/`SecurityError`, caller-relative origin state, guarded
  WindowProxy properties, and cross-origin `frameElement` handling;
- `crates/glass-browser/tests/native_engine.rs`: separate-origin HTTP frame
  witness covering parent and selected-child realms;
- native-engine architecture, analysis, plan, and issue records: synchronized
  production-gate status.

## Verification

- focused cross-origin frame security witness: 1 passed;
- full native integration target after the checkpoint;
- `cargo fmt --all -- --check` and `git diff --check`.

The slice closes one cross-origin isolation defect; it does not claim complete
Window Web IDL descriptors, live cross-realm DOM identity, full frame
lifecycle/load ordering, or native browser-complete promotion.
