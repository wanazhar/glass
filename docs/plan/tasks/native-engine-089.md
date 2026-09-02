---
id: native-engine-089
scope: glass-browser/native-engine/align-self-normal
status: complete
depends-on: [native-engine-088]
---

# Native bounded align-self normal

## Objective

Extend the bounded non-inherited `align-self` grammar with the explicit
`normal` keyword for eligible direct flex items. In the supported row and
row-reverse flex context, an explicit `align-self:normal` must reuse the
completed stretch used-size and complete-artifact owners while remaining
distinct from omitted `align-self:auto` in computed style and cascade.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [CSS Box Alignment Level 3: flex self-alignment](https://drafts.csswg.org/css-align/)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the explicit, non-inherited item value
`align-self:normal` alongside the completed `auto`, `flex-start`, `center`,
`flex-end`, and `stretch` values. Parsing remains case-insensitive and accepts
one token only. Computed style retains `Normal` as a distinct value;
`align-self:auto` remains the only omitted/default value and continues to
resolve from the parent's computed `align-items` at the existing placement
boundary.

For an eligible direct element child of the existing fixed-width row or
row-reverse flex layout, explicit `align-self:normal` uses the same used-value
behavior as explicit `align-self:stretch`:

- an omitted `height` fills the existing line cross size minus vertical
  margins, never shrinking below the natural height from the shared child
  layout pass;
- physical padding and border insets, bounded pixel `min-height`, and bounded
  pixel `max-height` constrain the resulting outer box through the existing
  box-model owner;
- single-row explicit parent content height and wrapped formed line height
  after line-gap and `align-content` distribution remain authoritative;
- an explicit bounded `height` is preserved and uses the bounded flex-start
  placement fallback.

An explicit `align-self:normal` overrides every parent `align-items` value in
the supported context. It therefore stretches an auto-height item even when
the parent is `center`, `flex-end`, or the omitted native `flex-start`
fallback. In contrast, `align-self:auto` remains parent-controlled, and
explicit `align-self:flex-start|center|flex-end|stretch` retains each existing
behavior. A child value is not inherited from its parent; the parent's
`align-items` value remains its own computed property.

Cascade specificity, source order, inline precedence, non-inheritance, and
invalid-declaration retention remain unchanged. A valid `normal` declaration
wins at its existing cascade position; an invalid later declaration does not
erase an earlier valid item value. The value remains observable in computed
style and does not introduce a new diagnostic for supported flex use.

Outside the bounded row/row-reverse flex layout, this slice does not claim
general block, grid, absolute-positioned, writing-mode, logical-axis,
baseline, safe/unsafe, auto-margin, column-direction, or intrinsic-sizing
semantics for `normal`. CSS-wide keywords, multi-token forms, fractional,
percentage, and unsupported values remain typed diagnostics or out of scope.
No parent `align-items`, `align-content`, `place-content`, line-formation,
main-axis, or initial-value behavior changes.

The resulting layout box and complete descendant artifact range must remain
consistent across line overflow, display-list paint, software rasterization,
viewport projection, hit testing, scrolling, capture, and semantic/source
order. The slice reuses the existing stretch helper and introduces no second
cross-axis geometry representation.

## Tradeoffs

- A distinct computed `Normal` value preserves specified-value provenance and
  allows future layout-mode-specific behavior, at the cost of one explicit
  used-value mapping in the flex owner.
- Reusing the completed stretch path gives explicit `normal` and `stretch`
  one geometry/artifact owner. The bounded mapping intentionally does not
  pretend to implement `normal` for every CSS layout mode.
- Explicit heights remain fixed and top-aligned under the existing bounded
  fallback. This protects author-declared dimensions while declining full CSS
  used-value parity for all cross-size combinations.
- Physical integer-pixel padding, borders, and min/max dimensions remain the
  only supported constraints. Logical properties, fractional metrics, real
  font metrics, and writing modes remain outside the native boundary.
- No new dependency, crate, runtime, or artifact pipeline is added; the
  native backend remains default-off inside `glass-browser`, and Chromium/CDP
  remains the production runtime.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

- Design checkpoint: `95caf4a9`.
- Implementation checkpoint: `1ee55c43`. Focused CSS parser/cascade
  coverage passed 2/2; the clean isolated measurement was 814.44 seconds with
  2,511,600 KiB peak RSS.
- Focused shared-layout integration passed 1/1 in the final run; its
  incremental isolated measurement was 26.54 seconds with 612,284 KiB peak
  RSS. The fixture covers explicit normal-to-stretch equivalence, overriding
  parent alignment, auto-height and explicit-height behavior, box-model
  bounds, descendants, paint, and hit testing. Two earlier fixture assertions
  were corrected during validation: the existing text line box centers at
  `y=6`, and the fourth item begins at `x=30`; neither exposed a
  production implementation failure.
- The full native integration suite passed 116/116 in 3.63 seconds with
  82,864 KiB peak RSS. The first full native library run reached 886 passed
  and one failure in the known environment-sensitive Rust Analyzer
  diagnostics-cancellation probe; its exact isolated retry passed 1/1 in
  1.14 seconds with 82,780 KiB peak RSS. The complete library retry then
  passed 887 tests with 1 ignored and 0 failures in 4.58 seconds with
  82,728 KiB peak RSS.
- Strict all-feature Clippy passed with warnings denied in 809.68 seconds with
  1,881,692 KiB peak RSS. Strict no-default-feature Clippy passed in 437.17
  seconds with 1,798,152 KiB peak RSS. Workspace rustdoc with
  `RUSTDOCFLAGS="-D warnings"` passed in 193.47 seconds with 1,631,548 KiB
  peak RSS. The locked `glass-dev` binary build passed in 679.11 seconds
  with 1,994,292 KiB peak RSS.
- Formatting, `git diff --check`, version synchronization at `0.3.14`,
  feature parity (14 capabilities across 4 targets), release documentation
  (503 Markdown files, 83 current documents, 57 previous-version hits, 562
  semantic audit hits, and 0 current-claim failures), TUI shortcut inventory
  (15 implementation keys and 63 documentation markers), documentation depth
  (93 current guides and 19 substantive contracts), reliability (6 scenarios
  across 4 targets), public read-only adapters (5), and Web IR (8 fixtures,
  8 scenarios, and 11 categories with runtime goldens) all passed.
- Live documentation coverage passed in 4.58 seconds with 38,292 KiB peak
  RSS: 503 Markdown files, 345 full-product MCP tools (100 browser-only),
  17 examples, and 22 public modules.
- The exact temporary `/tmp/glass-089-target` tree reached 5.3G during
  validation. It was inspected after all processes exited, had no open files,
  and was removed in full. The project
  `/home/ubuntu/work/glass/target` remains only its root directory at 4.0K;
  `/dev/sda1` is 129G used, 65G available, and 67% full. Shared Cargo
  registries and toolchains were retained, and long-lived Glass processes
  were not terminated.
- Issue [#40](https://github.com/wanazhar/glass/issues/40) was updated with
  the design, implementation, evidence, and cleanup state. Remote CI is not
  claimed because this branch remains local-only; no push, release, tag,
  registry publication, or browser-parity certification is claimed.
