---
id: native-engine-084
scope: glass-browser/native-engine/align-self
status: active
depends-on: [native-engine-083]
---

# Native bounded align-self override

## Objective

Add the small `align-self` item-level override needed by the existing bounded
flex cross-axis owner. The slice must preserve one computed-style and layout
owner, keep `auto` tied to the flex container's resolved `align-items` value,
and move complete item subtrees through the existing artifact translation
path.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts one ASCII-whitespace-trimmed,
case-insensitive `align-self` keyword:

- `auto`, resolving to the flex container's computed `align-items` value;
- `flex-start`, `center`, or `flex-end`, overriding the container for that
  direct flex item.

The property is non-inherited and defaults to `auto`. `stretch`, `baseline`,
`normal`, logical `start`/`end`, `safe`/`unsafe` combinations, CSS-wide
keywords, multi-token values, and other unsupported forms remain typed
diagnostics and do not erase an earlier valid declaration.

Valid declarations use the existing specificity, source-order, and inline
precedence. A later valid declaration replaces the item property; an invalid
declaration leaves the prior winner unchanged. `auto` is resolved only when an
eligible flex row is laid out, against the already-computed parent
`align-items` value; it does not copy or inherit the parent's computed style
into the child.

Only eligible direct element children of the existing bounded fixed-width flex
row consume the value. The chosen alignment reuses the current line height,
margin accounting, complete subtree box/text artifact ranges, display-list,
raster, overflow, projection, hit-test, scroll, capture, and semantic/source
order consumers. It does not change line formation, main-axis sizing, flex
growth/shrink/basis, or `align-content`.

## Tradeoffs

- Supporting `auto` plus the three physical flex-edge values gives common
  per-item overrides while keeping the layout change to one existing offset
  decision. The native claim intentionally excludes stretch sizing and real
  baseline metrics.
- `auto` is resolved at the flex parent rather than inherited through the DOM,
  preserving CSS's item-local behavior and preventing a non-flex descendant
  from accidentally becoming aligned.
- The value is stored in the existing computed-style/cascade representation and
  carried into the existing placement records. This avoids a second alignment
  pass, at the cost of rejecting logical/writing-mode and browser-parity forms.
- Because the line box remains owned by the parent, an item override cannot
  change the line's cross size. That deliberately leaves stretch, auto-margin,
  baseline, column direction, and intrinsic/fractional metrics for later
  contracts.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The implementation, focused parser/cascade and shared-layout tests, full native
suites, strict feature/documentation gates, issue #40 status, and exact
regenerable-target cleanup will be recorded here when the slice closes. Remote
CI is not claimed until the local branch is pushed.
