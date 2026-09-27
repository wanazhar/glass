---
id: native-engine-browser-773
scope: glass-browser/native-explicit-tabindex-focus
status: completed
depends-on: [native-engine-browser-772]
---

# Glass native-engine browser slice 773: explicit `tabindex` on generic elements

## Objective

Extend native sequential and programmatic focus to rendered DOM elements with
an explicit valid `tabindex`, even when their semantic role is not one of the
currently actionable controls.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for browser completion.
- The [Glass Core Web Profile](../native-engine-browser-profile.md) defines
  the bounded explicit-`tabindex` focus baseline.
- [Slice 772](native-engine-browser-772.md) implements image-map area focus
  through per-image DOM anchors; that special path must remain intact.
- The [HTML Standard `tabindex` contract](https://html.spec.whatwg.org/multipage/interaction.html#the-tabindex-attribute)
  distinguishes programmatic focusability from sequential focus order.

## Contract

- In the current light-DOM scope, an attached, rendered element with a valid
  explicit `tabindex` is programmatically focusable regardless of semantic
  role. A negative value permits programmatic focus but excludes the element
  from sequential navigation.
- Explicit nonnegative values join Tab and Shift+Tab navigation. Positive
  values sort ascending before zero; ties and zero use document tree order.
  The order is recomputed from the live document for each traversal.
- Missing or invalid `tabindex` does not make an otherwise generic element
  focusable. Existing implicit native-control focus remains unchanged.
- Actually disabled supported native controls and non-rendered targets are excluded. A
  `disabled` attribute on an otherwise generic element does not disable it.
  Image-map `area` elements continue using their associated image anchors.
- Keydown/keyup are delivered to the focused generic element. Enter does not
  synthesize a click for an element without native activation behavior.
- Local, HTTP(S) content-process, and same-origin-frame routes share the
  behavior.

## Boundaries

This does not implement `HTMLElement.tabIndex` reflection, focus options,
negative-tabindex sequential starting-point rules, shadow-root focus scopes,
inertness, form-associated custom-element disabled focus state, platform focus
preferences, complete focus-chain behavior, or full keyboard/WPT conformance.
It is not issue #40 completion evidence.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-773.md`

## Verification

Focused regressions cover local positive-index sorting and ties, zero/tree
order, reverse traversal, live `tabindex` mutation, generic `disabled`
attribute handling, and exclusion of negative, invalid, hidden, hidden-input,
and actually disabled candidates. A negative-index generic element receives
programmatic focus and Enter key events without a synthesized click. The
HTTP(S) content-process regression verifies top-level and same-origin-frame
ordering, negative-index programmatic focus, hidden/invalid exclusion on wrap,
frame-local keyboard targets, and Enter non-activation.

Checks passed:

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine --locked --quiet -- generic_explicit_tabindex_focus --test-threads=1` (both focused regressions)
- `cargo fmt --all -- --check`
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-773.json`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`

Workspace/all-target tests, Web Platform Tests, remote CI, platform
certification, full focus-chain conformance, and issue #40 completion remain
out of scope for this slice.
