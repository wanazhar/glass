---
id: native-engine-022
scope: glass-browser/native-engine/line-height
status: done
depends-on: [native-engine-021]
---

# Native bounded fixed line height

## Objective

Give the bounded inline-flow model an explicit, deterministic line-height
control without claiming font metrics or general CSS inheritance:

- parse one positive integer-pixel `line-height` value;
- cascade it with the existing selector/source-order/inline precedence;
- use the flow owner's value as the minimum line-box height for direct text and
  inline children; and
- make an inline element's own line-height the minimum of its auto content box
  height while preserving explicit `height` precedence.

This is a fixed-pixel line-box slice. It does not add `normal`, unitless,
percentage, `inherit`, `initial`, font-size-relative, font shaping, baseline,
vertical-align, or browser line-metric behavior.

## Contract

`line-height` accepts exactly one positive `<N>px` token within the existing
native dimension limit. Unsupported values are ignored without replacing an
earlier valid declaration. Stylesheet and inline values use the same bounded
specificity, source-order, and inline precedence rules as the other supported
presentation properties.

When laying out an element's children, its computed line-height becomes the
flow cursor's minimum line height; direct text and inline element boxes use at
least that height when a line is flushed. For an inline element without an
explicit height, its own computed line-height is also the minimum auto content
height. Explicit content-box or border-box `height` remains authoritative.
The value is local to the flow/element that owns it; this checkpoint does not
implement general CSS inheritance or font metrics.

The existing block/inline wrapping, box model, rounded hit testing,
display-list generation, software replay, root scrolling, revision behavior,
default-off feature gate, and two-crate boundary remain unchanged.

No stable backend capability, dependency, third crate, automatic backend path,
or screenshot evidence contract changes.

## Tradeoffs

- A fixed pixel floor makes dense and spacious fixture layouts reproducible
  without bringing in a font or shaping stack.
- Rejecting `normal`, relative, and percentage values avoids pretending that a
  line box has browser font metrics, but limits real document fidelity.
- Applying the floor at the flow owner keeps adjacent inline wrapping simple;
  local element line-height can enlarge its own auto box, but values are not
  generally inherited through the DOM.
- Explicit height precedence preserves the existing box-model contract, even
  where full CSS would distinguish line box metrics from border-box sizing.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- native-engine unit tests and synchronized capability documentation

## Verification

- `cargo fmt --all -- --check`
- focused CSS cascade and layout/display-list/hit-test tests;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage/depth/release-truth validators;
- `git diff --check` and one focused Conventional Commit.

## Completion evidence

Implemented and verified locally. `line-height` now accepts one positive
integer-pixel value through the existing bounded cascade, supplies the owning
flow cursor's minimum line height, raises inline auto-height, and preserves
explicit height precedence. The tests cover unsupported values, stylesheet
and inline precedence, wrapped line placement, display-list origins, and
depth-aware hit testing.

- `cargo fmt --all -- --check` and `git diff --check` pass.
- Native unit tests: 33 passed.
- Native integration tests: 31 passed, including fixed line-height flow and
  explicit-height behavior.
- Strict default-feature and `native-engine` Clippy gates pass.
- Explicit default and native library checks pass.
- Full locked `glass-browser` all-target/all-feature matrix: 815 passed, 1
  ignored.
- Documentation coverage: 436 Markdown files, 345 full-product MCP tools,
  100 browser-only tools, 17 examples, and 22 public modules.
- Documentation depth: 93 current guides and 19 substantive contracts.
- Release-truth audit: 0 current-claim failures.
- Unit-pixel-only and no-general-inheritance limitations remain explicit; no
  stable transport capability, dependency, third crate, or automatic backend
  path changed.
