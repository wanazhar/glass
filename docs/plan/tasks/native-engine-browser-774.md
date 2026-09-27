---
id: native-engine-browser-774
scope: glass-browser/native-tabindex-idl-reflection
status: completed
depends-on: [native-engine-browser-773]
---

# Glass native-engine browser slice 774: `tabIndex` IDL reflection

## Objective

Expose the native element `tabIndex` property with HTML getter defaults and a
Web IDL `long` setter that mutates the same `tabindex` attribute consumed by
native focus traversal.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for browser completion.
- The [Glass Core Web Profile](../native-engine-browser-profile.md) requires
  browser-compatible DOM focus and mutation behavior.
- [Slice 773](native-engine-browser-773.md) routes explicit attributes through
  local, HTTP(S) content-process, and same-origin-frame focus owners.
- The [HTML Standard `tabIndex` getter and focus model](https://html.spec.whatwg.org/multipage/interaction.html#the-tabindex-attribute)
  defines signed-long parsing, element-specific fallback values, and the
  property behavior used by this slice.

## Contract

- A present `tabindex` attribute that parses within the signed 32-bit `long`
  range is returned as a number. Missing, invalid, or out-of-range attributes
  fall back to zero for `a`, `area`, `button`, `frame`, `iframe`, `input`,
  `object`, `select`, and `textarea`, and for the first `summary` child of a
  `details`; other elements return negative one.
- Assigning `element.tabIndex` uses Web IDL signed-`long` conversion, including
  truncation and modulo wrapping, then writes the canonical decimal value to
  the live `tabindex` content attribute. The Rust document sees the mutation
  through the existing attribute command; subsequent Tab order uses that
  updated value.
- Direct `setAttribute()`/`removeAttribute()` calls are immediately reflected
  by the getter, including invalid-value fallback. The property works on
  current native HTML/SVG element projections, detached elements, and frame
  projections that share the common property owner.
- Local, HTTP(S) content-process, and same-origin-frame routes preserve the
  property, attribute, and focus-order behavior.

## Boundaries

This does not implement all Web IDL interfaces or property descriptors,
complete arbitrary-precision HTML integer parsing, focus options, shadow-root
focus scopes, or full focus/WPT conformance. It is not issue #40 completion
evidence.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-774.md`

## Verification

The local regression verifies implicit defaults for generic/link/button/
summary/hidden-input elements, invalid and out-of-range attribute fallback,
signed-long truncation and wraparound on assignment, canonical attribute
serialization, and that a property-created positive `tabindex` enters the
Rust-owned Tab order. The HTTP(S) regression verifies property assignment and
readback across script turns, top-level focus order, and setter-backed focus
ordering in a same-origin child frame.

Checks passed:

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine --locked --quiet -- tab_index_property --test-threads=1` (2 focused regressions)
- `cargo fmt --all -- --check`
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-774.json`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`

Workspace/all-target tests, Web Platform Tests, remote CI, cross-platform
certification, and issue #40 completion remain out of scope for this slice.
