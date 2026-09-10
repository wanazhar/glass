---
id: native-engine-browser-145
scope: glass-browser/native-engine/child-frame-ownership
status: completed
depends-on: [native-engine-browser-144]
---

# Native engine browser slice 145: child-frame ownership

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Give the native runtime a real, target-owned browsing-context tree. A frame
must have an independently live native document owner while the selected
frame receives the existing Glass navigation, evidence, action, script,
storage, prompt, download, wait, and capture operations.

## Contract

- every selected native page target starts with a stable `target:main` frame;
- `iframe` and `frame` owners are discovered in document order, including
  nested descendants, with deterministic `target:frame-N` IDs;
- an omitted or empty `src` creates an `about:blank` child document;
- `srcdoc` creates a bounded native data document from the owner’s source;
- frame records expose stable IDs, parent IDs, redacted URLs, exactly one
  active frame, and `out_of_process=false` for native internal ownership;
- child documents are initialized before they are published by
  `listFrames`, and failed initialization never produces a half-live frame;
- selecting a frame swaps its complete native engine owner while parking the
  previously selected frame and retaining its document, realm, history,
  storage, prompts, downloads, request ledger, and revision state;
- navigating a selected child updates that child only and re-discovers its
  descendant frame tree without resetting its parent or sibling owners;
- selecting a parent or descendant routes normal Glass operations to that
  selected owner and reports the selected frame in semantic inspection;
- frame topology is bounded by the existing `TOPOLOGY_MAX_FRAMES` limit and
  fails closed when the limit, source URL, or initialization contract fails;
- closing a target drains its selected and parked frame engines; session close
  drains every target and frame owner without Chromium/CDP fallback;
- the runtime, native CLI/MCP topology surfaces, and integration tests use the
  same backend owner; no frame is represented by metadata alone.

## Tradeoffs

The implementation parks complete `NativeEngine` instances and swaps the
single active engine lock already used by the backend. This keeps all existing
revision-bound operations frame-correct without adding a frame parameter to
every Glass API, at the cost of serializing frame selection and active-frame
work. It also spends more memory per live frame, but preserves execution state
instead of replaying navigation and losing page mutations.

Native child frames are internal engine owners rather than Chromium-style
out-of-process iframes, so the public projection correctly reports
`out_of_process=false`. Network child documents still use the existing
sandboxed content worker owned by their `NativeEngine`; local documents remain
on the deterministic local path. CSP frame-source enforcement, frame load/
unload event parity, shared browsing-context scripting, postMessage,
same-origin DOM access across frames, popup-default-action creation, and
complete resource lifecycle parity remain subsequent issue #40 gates.

## Implementation surface

- `browser/native_engine/dom.rs`: frame-owner discovery, fallback-content
  exclusion, and bounded `srcdoc` data-document encoding;
- `browser/native_engine/engine.rs`: stable frame identity, document
  generation, source resolution, and frame-owned download records;
- `browser/native_backend.rs`: frame registry, nested discovery, explicit
  selection, active-frame routing, parent-before-child projection, and
  cleanup;
- `browser/runtime.rs`: async frame lifecycle and selected-frame inspection
  routing;
- `tests/native_engine.rs`: nested discovery, `srcdoc`, child navigation,
  script/action routing, selection retention, and parent restoration.

## Verification

```text
cargo fmt --all
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine native_runtime_session_owns_and_routes_child_frames -- --nocapture
```

The focused native integration test passes with a parent, a navigated child,
and a `srcdoc` grandchild. Full native integration/library, strict affected
package lint, workspace gates, and remote CI remain required before issue #40
production promotion.
