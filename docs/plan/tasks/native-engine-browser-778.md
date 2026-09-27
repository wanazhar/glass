---
id: native-engine-browser-778
scope: glass-browser/focus-event-handler-idl
status: completed
depends-on: [native-engine-browser-777]
---

# Glass native-engine browser slice 778: focus event-handler IDL properties

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML Standard's `GlobalEventHandlers`](https://html.spec.whatwg.org/multipage/webappapis.html#globaleventhandlers)
  includes `onfocus` and `onblur` for HTML elements as event-handler IDL
  attributes. The engine already dispatches these events, but ordinary native
  elements did not expose the corresponding properties.
- Slice 777 supplies event order and related-target metadata; this slice wires
  the existing dispatch path to the element property API without introducing
  another event owner.

## Objective

Expose bounded `onfocus` and `onblur` handler properties on ordinary HTML
elements and their same-origin parent-frame projections, using the current
native event-listener ownership path.

## Contract

- New ordinary HTML elements expose `onfocus` and `onblur`, initially `null`.
- Assigning a function registers it for the corresponding event with the
  event's current target as `this`; replacing a function changes the handler
  without moving its listener slot relative to `addEventListener` registrations.
  Assigning `null` clears it and removes the native listener.
- Local documents and HTTP(S) same-origin child-process/frame projections use
  the same event-handler property semantics and target identity. Handler
  storage is keyed by the stable event owner, so a refreshed frame projection
  reads, replaces, and clears the existing handler instead of leaving an
  obsolete callback attached to a discarded projection object.
- Tests cover replacement, cleared-property reads, handler `this` and target,
  listener order, and subsequent focus transitions in local and process/frame
  routes.

## Boundaries and tradeoffs

This is the IDL-property path only. Inline `onfocus`/`onblur` content-attribute
compilation, `body`/`frameset` Window-target remapping, the remaining
`GlobalEventHandlers` properties, complete event-handler return-value behavior,
and full focus Web IDL/reentrancy conformance remain separate requirements.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-778.md`

## Verification

Passed:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --lib --locked --quiet`
- `cargo test -p glass-browser --test native_engine --locked --quiet -- focusin_focusout --test-threads=1`
  (2 local and HTTP(S) same-origin-frame process/parent-projection regressions)
- `python3 scripts/check-release-documentation.py --require-previous-version`
  (1,406 Markdown documents, zero current-claim failures)
- `python3 scripts/check-documentation-depth.py`
  (93 current guides / 19 substantive contracts)
- `python3 scripts/check-tui-shortcuts.py`
  (15 implementation keys / 63 documentation markers)
- `python3 scripts/check-documentation-coverage.py`
  (1,406 Markdown files)
- `git diff --check`

Full WPT, remote CI, cross-platform certification, and issue #40 completion
remain open.
