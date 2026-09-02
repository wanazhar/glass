---
id: native-engine-084
scope: glass-browser/native-engine/align-self
status: complete
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

- Design checkpoint: `36085e9c`.
- Implementation checkpoint: `f2f99f66`.
- Focused CSS parser/cascade coverage passed 2/2 tests in 1m23s after the
  invalid-later-declaration case was corrected to preserve the earlier valid
  winner.
- Focused shared-layout integration coverage passed 1/1 test in 22s for
  cross-axis positions, nested subtree translation, paint, and hit testing.
- Full native integration passed 110/110 tests in 3s; the full native library
  suite passed 885 tests with 1 ignored under `RUST_MIN_STACK=8388608`.
- Strict all-feature Clippy passed with warnings denied in 13m41s; no-default-
  feature Clippy passed with warnings denied in 6m45s.
- Rustdoc passed with `RUSTDOCFLAGS='-D warnings'` in 3m19s; the locked
  `glass-dev` build passed in 11m36s.
- Formatting, whitespace, version sync, feature parity, release-documentation,
  TUI, documentation-depth, documentation-coverage, reliability, public
  read-only adapter, and Web IR validators all passed. The live counts were:
  version `0.3.14`; feature parity 14 capabilities across 4 targets; 498
  Markdown documents with 0 current-claim failures; TUI 15 implementation
  help keys and 63 documentation markers; depth 93 guides and 19 contracts;
  coverage 498 Markdown files, 345 full-product MCP tools (100 browser-only),
  17 examples, and 22 public modules; reliability 6 scenarios across 4
  targets; 5 public read-only adapters; and Web IR 8 fixtures, 8 scenarios,
  and 11 categories.
- Issue #40 was updated with the implementation checkpoint, evidence, and
  local-only boundary. Remote CI remains unclaimed because this branch is not
  pushed.
- After all validation, no Cargo/Rust writer, open target handle, or Git lock
  remained. Exact regenerable `/home/ubuntu/work/glass/target` output fell
  from 5.7G to 4.0K; `/dev/sda1` moved from 134G used/60G available/70% to
  128G used/65G available/67%. Shared registries/toolchains and long-lived
  Glass processes were retained.
