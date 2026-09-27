---
id: native-engine-browser-781
scope: glass-browser/window-body-inline-event-handlers
status: completed
depends-on: [native-engine-browser-780]
---

# Glass native-engine browser slice 781: body and frameset inline event handlers

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML Standard event-handler algorithms](https://html.spec.whatwg.org/multipage/webappapis.html#event-handlers-on-elements,-document-objects,-and-window-objects)
  resolve selected `body` and `frameset` handlers to the associated Window.
  Content-attribute changes and IDL property assignments operate on that
  target's shared event-handler entry, retaining one listener as its value is
  replaced.
- Slice 780 implemented the IDL target mapping, but the generic inline
  `on*` content-attribute path still installed a separate listener on the
  element.

## Objective

Route the already-mapped `body` and `frameset` event-handler content attributes
through their owning Window's shared event-handler entry, retaining CSP
authorization and listener ordering while leaving ordinary element handlers
on their current element-owned path.

## Contract

- A body/frameset content attribute whose event type is already Window-routed
  by slice 780 targets that element's owning Document `defaultView`.
- The content attribute, the corresponding body/frameset IDL property, and
  the Window IDL property read and replace one handler value and one listener
  registration. Replacing the handler value does not move the listener in
  event-listener order.
- Removing a routed content attribute deactivates that target/name entry.
- `script-src-attr` authorization remains evaluated against the element and
  exact source text before a routed content-attribute value is installed. A
  CSP-blocked value does not replace an already-active routed handler.
  Existing non-routed element content attributes retain their established
  behavior.
- `this` for a routed content-attribute handler is the Window current target;
  the special Window `onerror` callback argument shape follows the existing
  Window event-handler invocation path.
- An unrelated projection refresh with an unchanged attribute does not
  deactivate/reactivate its shared Window listener or change its order.
- Local and process-backed tests cover body and frameset aliasing and dispatch,
  IDL replacement and removal, listener order, CSP-approved and blocked
  dynamic values, unchanged body-local `onclick`, and same-origin frame Window
  ownership.

## Boundaries and tradeoffs

- This reuses the current native event-handler map and CSP hook; it does not
  introduce a second Window listener or weaken inline-script policy.
- Complete inline-handler lexical environments and source locations, all
  `Window.onerror` reporting/cancellation rules, `beforeunload` prompting,
  event lifecycle timing, and full event-handler Web IDL/WPT conformance remain
  separate profile work.
- Cross-origin WindowProxy policy, process isolation, remote CI,
  cross-platform certification, and issue #40's native-only production gates
  remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-781.md`

## Verification

Passed locally:

- `rustfmt --edition 2024 --check crates/glass-browser/tests/native_engine.rs`
- `cargo check -p glass-browser --test native_engine --locked --quiet` (existing
  parser/dead-code warnings only)
- `cargo test -p glass-browser --test native_engine --locked --quiet -- focusin_focusout --test-threads=1`
  (2 passed; local and process-backed same-origin frame coverage)
- `cargo test -p glass-browser --test native_engine --locked --quiet -- native_http_focusin_focusout_persist_in_same_origin_frame --exact --test-threads=1`
  (1 passed)
- `cargo test -p glass-browser --test native_engine --locked --quiet -- native_content_process_routes_body_and_frameset_handlers_through_window_slot --exact --test-threads=1`
  (1 passed; includes an intervening evaluation/snapshot refresh)
- `python3 scripts/check-documentation-coverage.py` (1,409 Markdown files)
- `git diff --check`

Remote CI, cross-platform certification, full WPT, and issue #40's
native-only production gates were not run and remain open.
