---
id: native-engine-browser-780
scope: glass-browser/window-document-event-handler-idl
status: completed
depends-on: [native-engine-browser-779]
---

# Glass native-engine browser slice 780: Window and Document event-handler IDL

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML Standard event-handler tables and IDL](https://html.spec.whatwg.org/multipage/webappapis.html#globaleventhandlers)
  require the `GlobalEventHandlers` IDL set on `Window` and `Document`, the
  `WindowEventHandlers` set on `Window`, and `onreadystatechange` plus
  `onvisibilitychange` on `Document`. `body` and `frameset` expose the window
  handler set and route its IDL handler properties, plus `blur`, `error`,
  `focus`, `load`, `resize`, and `scroll`, to their owning Document's
  `defaultView`.
- Slice 779 added the common element-side `GlobalEventHandlers` property set
  and owner-keyed event-handler state, but left the Window/Document surfaces
  and body/frameset Window routing open.

## Objective

Implement the current Window and Document event-handler IDL property surfaces
and the IDL target mapping for body/frameset while preserving the existing
per-owner registration, replacement-order, and frame-projection behavior.

## Contract

- Native Window objects expose all current `GlobalEventHandlers` and
  `WindowEventHandlers` event-handler IDL properties, initially null.
- Native Document objects expose all current `GlobalEventHandlers` properties
  plus `onreadystatechange` and `onvisibilitychange`, initially null.
- HTML `body` and `frameset` elements expose `WindowEventHandlers` properties.
  Those names and the six Window-reflecting GlobalEventHandlers names resolve
  through the element's owning Document `defaultView`; other common event
  handlers remain owned by the element.
- `HTMLFrameSetElement` identity is exposed for native frameset elements.
- The target mapping uses the stable Window/WindowProxy event owner, so setting,
  reading, dispatching, replacing, and clearing work across a same-origin
  parent-frame projection without placing handler state on transient DOM
  projection objects.
- Local and process-backed tests verify the complete property surfaces,
  initial null values, representative Window/Document dispatch, body/frameset
  Window aliasing, element-local click handlers, and same-origin frame
  projection ownership.

## Boundaries and tradeoffs

- The existing generic inline `on*` content-attribute compiler still registers
  on its element owner; this slice does not route body/frameset content
  attributes through the Window handler map or establish their CSP parity.
- Event-handler sources and default actions are not expanded by adding IDL
  properties. Full Web IDL callback conversion, legacy `onerror` and
  `onbeforeunload` return-value behavior, event scheduling, and complete event
  conformance remain open.
- WindowProxy cross-origin restrictions, lifecycle, process boundaries, WPT,
  remote CI, cross-platform certification, and issue #40's production gates
  remain independent requirements.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-780.md`

## Verification

Passed:

- `rustfmt --edition 2024 crates/glass-browser/src/browser/native_engine/javascript.rs crates/glass-browser/tests/native_engine.rs`
- `cargo check -p glass-browser --test native_engine --locked --quiet`
  (existing parser/dead-code warnings only)
- `cargo test -p glass-browser --test native_engine --locked --quiet -- focusin_focusout --test-threads=1`
  (2 local and HTTP(S) same-origin-frame regressions; 57.70 seconds)
- `python3 scripts/check-documentation-coverage.py`
  (1,408 Markdown files; coverage validated)
- `git diff --check`

The first focused run exposed a main-Document event-owner mismatch: property
handlers registered under `node:undefined` while Document listener methods
used `document`. Assigning the main Document its stable internal event owner
fixed registration/dispatch alignment; the final focused rerun passed.

Remote CI, full Web Platform Tests, cross-platform certification, and issue
#40's native-only promotion gates were not run by this local slice.
