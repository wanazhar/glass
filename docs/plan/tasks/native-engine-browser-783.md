---
id: native-engine-browser-783
scope: glass-browser/event-handler-false-cancellation
status: in_progress
depends-on: [native-engine-browser-782]
---

# Glass native-engine browser slice 783: ordinary event-handler cancellation

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML Standard event-handler processing algorithm](https://html.spec.whatwg.org/multipage/webappapis.html#event-handlers-on-elements,-document-objects,-and-window-objects)
  sets the canceled flag when an ordinary handler returns exactly `false`.
  This is distinct from the special global `onerror` rule and the typed
  `onbeforeunload` return behavior.
- The current native wrapper applies `preventDefault()` only to content
  attributes that return false. That misses IDL properties and fails to
  reflect cancellation for non-cancelable events.

## Objective

Apply ordinary EventHandler `return false` cancellation uniformly to IDL and
content-attribute handlers without changing listener ordering, propagation,
Window `onerror`, or `onbeforeunload` behavior.

## Contract

- For an ordinary EventHandler callback, exactly `false` sets the event's
  canceled state, `defaultPrevented` is true, and `dispatchEvent()` returns
  false, whether or not `event.cancelable` is true.
- The rule is the same for IDL event-handler properties and event-handler
  content attributes; it does not depend on internal provenance metadata.
- Cancellation does not stop immediate/normal propagation: listeners after
  the handler still run and observe the canceled state.
- Values other than exactly `false` do not invoke ordinary cancellation.
- Slice 782's special Window `ErrorEvent` behavior remains distinct: exact
  `true`, not false, cancels when processing an `ErrorEvent` at Window.
- `onbeforeunload` remains on its separate typed return-value path and is not
  redefined by this slice. CSP and body/frameset shared-owner semantics remain
  unchanged.
- Local coverage exercises IDL and content-attribute callbacks on
  non-cancelable events, propagation after cancellation, non-false return
  values, and the special error handler regression.

## Boundaries and tradeoffs

- This does not implement `onbeforeunload` DOMString coercion/returnValue,
  callback exceptions through the DOM reporting algorithm, passive-listener
  rules, or all EventHandler Web IDL conversion behavior.
- Full WPT, remote CI, cross-platform certification, and issue #40's
  native-only production gates remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-783.md`

## Verification

Pending implementation and focused verification.
