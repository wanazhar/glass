id: native-engine-025
scope: glass-browser/native-engine/physical-box-edges
status: done
depends-on: [native-engine-024]
---

# Native bounded physical box edges

## Objective

Close the next documented box-model gap without importing a CSS layout
dependency:

- expand one-, two-, three-, and four-value `padding`/`margin` shorthands;
- accept bounded non-negative integer-pixel physical longhands for top, right,
  bottom, and left;
- cascade each physical side independently across stylesheet rules and inline
  style; and
- feed those sides consistently into content origins, normal flow, paint
  metadata, hit testing, and root-scroll projections.

This is a focused continuation of the uniform 016 box-model contract. It keeps
the two-installable-crate boundary and the `glass-browser` default-off native
feature unchanged.

## Contract

`padding` and `margin` accept only non-negative bounded `<N>px` values. The
shorthand expands using the CSS physical order:

```text
one       -> top right bottom left
two       -> top/bottom right/left
three     -> top right/left bottom
four      -> top right bottom left
```

The supported physical longhands are `*-top`, `*-right`, `*-bottom`, and
`*-left`. Stylesheet and inline declarations resolve each side independently
with the existing specificity, source-order, and inline-precedence rules.
Invalid values are ignored and do not erase an earlier valid declaration in
the same declaration block. Negative values, percentages, unitless values,
`auto`, more than four shorthand values, logical writing-mode properties,
margin collapsing, min/max constraints, positioning, flex, grid, and general
CSS layout remain outside this slice.

The layout owner retains the outer border-box rectangle and derives its content
rectangle by applying each physical border and padding side. Child and direct
text origins use the corresponding left/top insets. Block and inline normal
flow consume the corresponding margin sides; no margin collapsing is added.
All downstream display-list, software replay, root-scroll, and point-hit
consumers continue to use the same document-space geometry and revision.

## Tradeoffs

- Physical sides cover common local fixtures and make asymmetric cards and
  controls testable, but logical sides and writing modes remain unsupported.
- Four independent values improve geometry fidelity while retaining compact
  integer arithmetic; overflow is saturating and fractional CSS remains out
  of scope.
- Side-aware margins are deterministic normal-flow spacing, not a full CSS
  margin model: adjacent margins do not collapse and `auto` is rejected.
- Keeping the parser and layout code local avoids a dependency and compile-time
  cost, but the supported grammar must stay explicit and narrow.
- Every side is carried through existing derived layout consumers, which keeps
  hit/paint/scroll coordinates aligned but does not provide browser parity.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and capability docs

## Verification

- focused CSS parser/cascade and physical-box layout tests;
- full native integration and native unit suites;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage, depth, and release-truth validators;
- `cargo fmt --all -- --check` and `git diff --check`; and
- one focused Conventional Commit before the next slice.

## Completion evidence

Implemented and verified locally in the focused
`feat(native-engine): add physical box edges` checkpoint. The issue #40
comment/body records the resulting commit identity.

- Focused CSS parser/cascade tests: 13 passed.
- Native integration tests: 35 passed.
- Native unit tests: 35 passed.
- Strict default-feature and `native-engine` Clippy gates pass with warnings
  denied.
- Full locked `glass-browser` all-target/all-feature matrix: 817 passed, 1
  ignored; all integration suites passed, including 35 native integration
  tests.
- `cargo fmt --all -- --check` and `git diff --check` pass.
- Documentation coverage: 439 Markdown files, 345 full-product MCP tools,
  100 browser-only tools, 17 examples, and 22 public modules.
- Documentation depth: 93 current guides and 19 substantive contracts.
- Release-truth audit: 0 current-claim failures.
- No new dependency, third crate, stable transport capability, automatic
  backend path, or browser-parity/security claim was introduced.
