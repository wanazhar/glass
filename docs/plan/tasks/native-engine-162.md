---
id: native-engine-162
scope: glass-browser/native-engine/cascade-border-complete-current-color
status: complete
depends-on: [native-engine-161]
---

# Native bounded complete border `currentColor`

## Objective

Accept the exact case-insensitive `currentColor` keyword as the color
component of the existing complete physical border shorthand forms:
`border`, `border-top`, `border-right`, `border-bottom`, and `border-left`.
The accepted bounded forms are `Npx <painted-style> currentColor`,
`Npx none currentColor`, and `Npx hidden currentColor`. Preserve the existing
literal-color complete forms, omitted-component `none`/`hidden`, independent
width/style/color streams, and concrete public `NativeBorder` values.

This slice completes the deferred-color path for complete physical shorthands.
It does not add omitted width/style defaults, logical sides, CSS-wide reset
machinery, or general CSS color syntax.

## Context

`native-engine-161` added private `NativeBorderColorValue::CurrentColor` to
standalone physical `border-color` and its four physical color longhands. The
complete border parser now uses that same typed color path, so
`border: 2px solid currentColor` and the corresponding complete `none` and
`hidden` forms project deferred color through the existing independent
component streams. The complete declaration wrapper retains concrete public
values while this slice adds private deferred-color variants.

Normative references:

- <https://www.w3.org/TR/css-color-4/#currentcolor-color>
- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthand>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-161.md`
- `docs/plan/tasks/native-engine-160.md`

## Contract

### Declaration and cascade state

- Each of the five physical complete border shorthand names accepts the
  existing bounded `Npx <style> <literal-color>` form plus exact,
  case-insensitive `Npx <style> currentColor` when `<style>` is one of the
  existing painted styles, `none`, or `hidden`.
- Complete deferred-color declarations carry bounded width and, where
  applicable, style privately. They project `CurrentColor` into the existing
  independent border color stream at the same declaration order. A winning
  `none` or `hidden` still suppresses current non-table paint and preserves its
  existing private style sentinel; deferred width/color must not resurrect a
  no-paint side.
- Standalone `revert-layer`, omitted-component `none`/`hidden`, concrete
  complete values, physical color longhands, layer priority, specificity,
  source order, unlayered/inline precedence, same-block declaration order,
  repeated rollback, and valid-before-invalid preservation remain
  authoritative.
- Missing or extra components, malformed dimensions/colors, unsupported style
  tokens, other CSS-wide keywords, arbitrary omitted defaults, and unsupported
  color functions remain typed unsupported-value diagnostics. Complete
  `currentColor` does not change the bounded parser's rejection of unrelated
  syntax.

### Existing owners preserved

- `NativeBorderColorValue::CurrentColor` remains private declaration/cascade
  state until computed-style resolution. Public `NativeBorderSide` and
  `NativeBorder` continue to contain concrete `NativeColor`; no public enum,
  diagnostic payload, display-list field, raster schema, dependency, feature
  default, or crate boundary changes.
- Resolved complete borders continue through existing width/style composition,
  physical box insets, display-list commands, rounded masks, clipping, opacity,
  viewport projection, capture, software raster, point-hit, and
  semantic/source-order projection.
- The slice remains fixture-relative, horizontal-tb, integer-pixel, non-table,
  and feature-gated behind `native-engine`. It does not implement omitted
  `medium`/color inference, logical border sides, gradients, system
  colors, color spaces, percentages, animations, multiple origins,
  `!important` inversion, collapsed-table conflict resolution, or browser-wide
  CSS color/border conformance.

## Tradeoffs

- Separate private complete deferred variants preserve the existing public
  `NativeBorderSide` shape and keep no-paint `none`/`hidden` state distinct,
  while reusing the color stream and computed-color substitution proven in
  native-engine-161.
- Supporting all three current complete style families in one bounded slice
  avoids a syntax hole where painted, none, and hidden forms disagree, but it
  intentionally does not infer any omitted component defaults.
- Complete shorthand resolution still happens at the element computed-style
  boundary, so local/inherited `color` is read once and current-color values
  cannot leak into paint-time or public protocol state.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, product docs, and
  issue records

## Verification

The completed slice must cover:

- case-insensitive complete painted, `none`, and `hidden` `currentColor`
  parsing for all five physical shorthand names, concrete form preservation,
  omitted forms, standalone `revert-layer`, and typed rejection of malformed,
  incomplete, CSS-wide, unsupported, and unrelated forms;
- local, inherited, and bounded-black color substitution; independent
  width/style/color projections; layer/specificity/source-order/inline
  precedence; same-block order; repeated rollback; valid-before-invalid
  preservation; and private/public separation;
- current no-paint behavior for complete `none`/`hidden`, painted border
  geometry, display-list color/style, decoded raster, clipping, capture,
  point-hit, semantic/source order, and diagnostic behavior; and
- focused feature check/tests, full native integration/library tests, strict
  affected-package Clippy, rustdoc, paired-crate check/build, formatting,
  documentation, final static gates, and bounded regenerable-target cleanup.
  Remote CI remains unclaimed until an explicitly authorized push.

## Implementation

Implemented in `44b887c4` from design checkpoint `4272f0fa`.

- Added private complete deferred-color declaration variants for painted,
  `none`, and `hidden` physical shorthands without changing public border
  structs or artifact schemas.
- Reused the bounded case-insensitive `currentColor` parser and projected
  complete width, style, and color through independent cascade streams at the
  original declaration order. Explicit complete `none`/`hidden` remains
  no-paint in the current non-table engine.
- Added parser coverage for all five physical complete shorthand names,
  concrete-value preservation, case-insensitive forms, malformed/incomplete
  values, CSS-wide keywords, unsupported styles/colors, and extra components.
- Added integration coverage for local current color, all physical complete
  forms, painted styles, no-paint behavior, layout, hit testing, semantics,
  display-list output, and raster output.

## Evidence

- `cargo fmt --all` and `git diff --check` passed.
- Focused feature check passed:
  `RUST_MIN_STACK=16777216 CARGO_TARGET_DIR=/tmp/glass-162-focused cargo check -q -p glass-browser --features native-engine --tests --locked`.
- Focused border unit tests passed: 18/18. The existing current-color
  integration test passed: 1/1. The new complete-physical integration test
  passed: 1/1.
- Full native integration passed: 200/200. Feature-enabled library tests
  passed: 968 passed, 1 ignored.
- Strict affected-package Clippy passed with all targets/features and
  `-D warnings`; feature rustdoc passed with `RUSTDOCFLAGS=-Dwarnings`.
- Paired `glass-dev` check and build passed in the isolated target.
- Static gates passed: version sync 0.3.14; feature parity 14 capabilities
  across 4 targets; release docs 576 Markdown/current-claim failures 0; TUI
  15 implementation keys/63 documentation markers; documentation depth 93
  guides/19 contracts; reliability 6 scenarios/4 targets; adapters 5; Web IR
  8 fixtures/8 scenarios/11 categories; documentation coverage 576 Markdown,
  345 full-product MCP tools, 17 examples, and 22 public modules.
- `RUST_MIN_STACK=33554432 CARGO_TARGET_DIR=/tmp/glass-162-workspace bash
  scripts/check-rust-workspace.sh` passed for the all-target/all-feature
  workspace test matrix; successful output was redirected to the named log.
- Before cleanup, `/tmp/glass-162-focused` contained 5,923,680,256 bytes
  (9,259 files/1,201 directories), `/tmp/glass-162-workspace` contained
  5,040,406,528 bytes (6,215 files/694 directories), and the named log was
  163,840 bytes. No cargo/rustc/rustdoc/clippy process or open handle targeted
  these paths. Exact bounded deletion reclaimed 10,964,250,624 measured bytes;
  free space rose from 71,862,439,936 to 82,826,702,848 bytes. All three
  temporary paths and the repository `target/` were verified absent afterward.
- Remote CI, push, release, tag, registry publication, browser parity, and
  security-boundary claims remain unmade because this checkout is local-only.
