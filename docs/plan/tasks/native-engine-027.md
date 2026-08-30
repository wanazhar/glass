id: native-engine-027
scope: glass-browser/native-engine/overflow-hit-test-clip
status: done
depends-on: [native-engine-026]
---

# Native bounded overflow hit-test and projection clips

## Objective

Close the remaining consumer mismatch in the existing `overflow:hidden`
boundary:

- derive one bounded rectangular ancestor-clip projection from the current
  document and layout boxes;
- prevent point hit-testing from selecting descendants outside an
  `overflow:hidden` ancestor's content-visible rectangle; and
- make `viewport_rect_for` report the same clipped visibility that paint and
  interaction consumers observe.

This is a layout-consumer continuation of `native-engine-013`. It does not add
new CSS syntax, nested scrolling, or a third crate; it keeps the native engine
inside `glass-browser` behind the default-off feature.

## Contract

For each laid-out element, the native layout snapshot retains a bounded
rectangular intersection of the element's own and every ancestor's
`overflow:hidden` border-box rectangle. The rectangles remain in document
coordinates and are translated only at the existing root viewport boundary.

Point hit-testing rejects a candidate whose rounded box contains the point but
whose ancestor clip does not. `viewport_rect_for` intersects the element box
with that same clip before applying the root viewport projection. Nested hidden
ancestors are intersected in document order; empty intersections make the
descendant non-visible to these consumers. Existing rounded own-box hit
testing, paint clipping, source order, scroll mapping, and revision behavior
remain unchanged.

The clip is rectangular, matching the current `overflow:hidden` paint
contract. This slice does not implement visible overflow, `overflow:clip` or
axis-specific overflow, nested/smooth scrolling, rounded descendant clip
geometry, stacking contexts, transforms, or general CSS hit-testing.

## Tradeoffs

- Storing the clip projection with the derived layout keeps hit-testing and
  viewport inspection aligned with software paint, but adds one bounded clip
  value per layout box.
- Reusing document-space rectangles makes root scrolling deterministic and
  avoids a second coordinate system, while nested scroll containers remain
  unsupported.
- Applying the rectangular clip before the existing rounded own-box test fixes
  overflow interaction leaks without claiming CSS overflow or border-radius
  clip fidelity.
- The layout snapshot repeats the existing bounded ancestor walk rather than
  introducing a shared mutable clip tree; this is simpler and cheap at the
  current node limits, but not the future renderer's general clip-stack model.

## Path

- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, and capability docs

## Verification

- focused overflow layout, viewport-projection, hit-test, and root-scroll
  tests;
- full native integration and native unit suites;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- documentation coverage, depth, and release-truth validators;
- `cargo fmt --all -- --check` and `git diff --check`; and
- one focused Conventional Commit before the next slice.

## Completion evidence

Implemented and verified locally in the focused
`feat(native-engine): clip overflow hit testing` checkpoint. The issue #40
comment/body records the resulting commit identity.

- Focused overflow projection/hit-test test: 1 passed.
- Native integration tests: 38 passed.
- Native unit tests: 35 passed.
- Strict default-feature and `native-engine` Clippy gates pass with warnings
  denied.
- Full locked `glass-browser` all-target/all-feature matrix: 817 passed, 1
  ignored; all integration suites passed, including 38 native integration
  tests.
- `cargo fmt --all -- --check` and `git diff --check` pass.
- Documentation coverage: 441 Markdown files, 345 full-product MCP tools,
  100 browser-only tools, 17 examples, and 22 public modules.
- Documentation depth: 93 current guides and 19 substantive contracts.
- Release-truth audit: 0 current-claim failures.
- No new dependency, third crate, stable transport capability, automatic
  backend path, or browser-parity/security claim was introduced.
