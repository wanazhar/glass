---
id: native-engine-browser-779
scope: glass-browser/global-event-handler-idl
status: completed
depends-on: [native-engine-browser-778]
---

# Glass native-engine browser slice 779: element GlobalEventHandlers IDL

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [HTML Standard's `GlobalEventHandlers` IDL](https://html.spec.whatwg.org/multipage/webappapis.html#globaleventhandlers)
  requires its event-handler properties on all HTML elements. Before this
  slice, ordinary elements had `onfocus`/`onblur`; image and media elements had
  selected additional properties, but the common click/input/keyboard and
  remaining event-handler properties were missing.
- Slice 778 moved handler value and listener ownership to stable native event
  owners so refreshed same-origin frame projections can share replacements and
  clearing. This slice completes the element property surface on a shared
  prototype without allocating every handler slot as an own property on every
  element.

## Objective

Expose every current `GlobalEventHandlers` event-handler IDL property on native
HTML element instances through one shared prototype and the existing native
event owner.

## Contract

- Every event-handler name in the current `GlobalEventHandlers` IDL is present
  on ordinary HTML elements and initially reads as null.
- The properties are inherited from the common HTML element prototype; normal
  elements do not receive dozens of additional own properties.
- Assigning/replacing functions and clearing with null retains slice 778's
  owner-keyed state, handler `this`, event target, and listener-order
  semantics, including refreshed same-origin frame projections.
- Image and media event-handler properties use the same inherited surface;
  installing the common properties does not redefine or duplicate the prior
  element-specific listener slots.
- Local and same-origin process/frame tests check the full property-name
  surface and representative click/input delivery, clearing, listener order,
  and target identity.

## Boundaries and tradeoffs

This completes the element-side `GlobalEventHandlers` property names; it does
not claim every event source or default action is implemented. `Window` and
`Document` handler sets, `WindowEventHandlers`, `body`/`frameset` Window-target
remapping, full Web IDL receiver conversion, CSP/error parity for inline
content attributes, and complete event/WPT conformance remain separate work.
Sharing descriptors on a prototype keeps per-node memory bounded; listener
state remains allocated only after a handler is assigned.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-779.md`

## Verification

Passed:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --test native_engine --locked --quiet`
  (existing parser/dead-code warnings only)
- `cargo test -p glass-browser --test native_engine --locked --quiet -- focusin_focusout --test-threads=1`
  (2 local and HTTP(S) same-origin-frame process/parent-projection regressions)
- `python3 scripts/check-release-documentation.py --require-previous-version`
  (1,407 Markdown documents, zero current-claim failures)
- `python3 scripts/check-documentation-depth.py`
  (93 current guides / 19 substantive contracts)
- `python3 scripts/check-tui-shortcuts.py`
  (15 implementation keys / 63 documentation markers)
- `python3 scripts/check-documentation-coverage.py`
  (1,407 Markdown files)
- `git diff --check`

Full WPT, remote CI, cross-platform certification, and issue #40 completion
remain open.
