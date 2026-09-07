---
id: native-engine-165
scope: glass-browser/native-engine/cascade-color-current-color
status: complete
depends-on: [native-engine-164]
---

# Native bounded `color: currentColor`

## Objective

Accept the exact case-insensitive `currentColor` keyword for the existing
inherited `color` property. Resolve a winning local `color: currentColor`
declaration from the already-computed inherited color, or from the bounded
initial black fallback when no inherited color is present. Preserve the public
`Option<NativeColor>` value and every existing background, border,
text-decoration, glyph, display-list, capture, and raster consumer.

This slice closes the self-reference gap deliberately left outside
native-engine-163 and -164 without introducing a generic CSS value-dependency
graph.

## Context

The native engine now resolves `currentColor` for physical border colors,
`background-color`, and local `text-decoration-color` after the existing
element/inherited `color` owner is selected. The remaining common gap is the
`color` property itself. A local `color: currentColor` must not recursively
resolve against its own unresolved declaration: its bounded meaning here is
the inherited computed color, with the existing initial black fallback at the
root.

Normative references:

- <https://www.w3.org/TR/css-color-4/#currentcolor-color>
- <https://www.w3.org/TR/css-color-4/#the-color-property>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-164.md`
- `docs/plan/tasks/native-engine-163.md`

## Contract

### Declaration and cascade state

- `color` accepts every existing bounded literal/alpha color, exact
  case-insensitive `currentColor`, and standalone case-insensitive
  `revert-layer`.
- A private deferred color value participates in the existing named-layer,
  unlayered, inline, specificity, source-order, same-block, and
  valid-before-invalid cascade behavior. It resolves only after the parent
  computed style has supplied inherited color.
- A local concrete color remains the winning element color when selected. A
  local `currentColor` resolves to the inherited computed color; when that is
  absent, it resolves to the bounded initial black value. A missing local
  declaration retains the existing inherited `Option<NativeColor>` behavior,
  including `None` at an undeclared root.
- Because `currentColor` on `color` is resolved from the parent value, the
  implementation does not recurse through the local declaration and does not
  create a cyclic dependency. Descendants inherit the resulting concrete
  color through the existing DOM style walk.
- Malformed values, other CSS-wide keywords, gradients, image functions,
  system colors, color-space functions, percentages, arbitrary functions,
  custom properties, and unresolved declaration keywords remain typed
  unsupported-value diagnostics and do not replace an earlier valid
  declaration.

### Existing owners preserved

- `NativeColorValue::CurrentColor` (or an equivalent private declaration-only
  owner) remains private until computed-style resolution. Public computed style
  continues to expose `Option<NativeColor>`; display commands, capture bytes,
  raster schemas, dependencies, feature defaults, and crate boundaries remain
  unchanged.
- The resolved concrete color continues through existing glyph text paint,
  `background-color: currentColor`, border current-color, decoration
  current-color, clipping, opacity, scrolling, capture, software raster,
  point-hit, and semantic/source-order consumers without downstream keyword
  handling.
- The slice remains fixture-relative, horizontal-tb, integer-pixel,
  fixed-cell, non-table, and feature-gated behind `native-engine` inside
  `glass-browser`.

## Tradeoffs

- A private deferred variant keeps the public color schema stable and reuses
  the already-proven post-cascade resolution boundary. A generic dependency
  graph would add cycle and invalidation complexity outside this engine's
  bounded contract.
- Resolving a local `currentColor` from the parent computed value matches the
  needed self-reference behavior while making the root initial-black choice
  explicit. It preserves the distinction between an omitted root color
  (`None`) and an explicit root `currentColor` (`Some(black)`).
- The same concrete result feeds all current-color consumers, so one test can
  verify that the feature composes across glyph, background, border, and
  decoration paint without widening any artifact schema.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, product docs, and
  issue records

## Verification

The completed slice must cover:

- case-insensitive parser acceptance for literal, alpha, `currentColor`, and
  `revert-layer`, plus typed rejection of unsupported and unrelated forms;
- local concrete, local current-color, inherited, explicit root black
  fallback, omitted-root `None`, layer/unlayered/inline precedence,
  specificity, same-block order, and valid-before-invalid preservation;
- non-recursive descendant inheritance and composition of the resolved color
  through glyph, background, border, and decoration display/raster artifacts,
  including clipping, opacity, capture, point-hit, semantic/source order, and
  private/public separation; and
- focused feature check/tests, full native integration/library tests, strict
  affected-package Clippy, rustdoc, paired-crate check/build, packaging,
  formatting, documentation, final static gates, workspace all-target/
  all-feature testing, and bounded regenerable-target cleanup. Remote CI
  remains unclaimed until an explicitly authorized push.

## Implementation

Implemented in `f7b5fd4e` (`feat(native-engine): support color currentColor`)
from the docs-first design checkpoint `75544c39`. Synchronized current product
documentation is `01438316`.

- Added the private `NativeColorValue::{Color, CurrentColor}` declaration value
  and carried it through the existing bounded color candidate stream without
  changing public computed-style or artifact schemas.
- Accepted exact case-insensitive `color: currentColor` and resolved it from
  the inherited computed color after local cascade selection, using bounded
  black when no inherited color exists. The local declaration never resolves
  against itself.
- Preserved concrete local colors, `revert-layer`, named-layer/unlayered/inline
  precedence, specificity, source order, same-block order, valid-before-invalid
  behavior, omitted-root optional state, descendant inheritance, and all
  existing background, border, decoration, glyph, layout, capture, and raster
  consumers.
- Added parser, direct cascade, and end-to-end integration coverage for local,
  inherited, root, omitted, rollback, invalid-later, inline, concrete, and
  multi-artifact current-color cases. The integration fixture also checks
  layout, point hit testing, semantic/source order, display-list colors, PNG
  raster output, and typed diagnostics.

## Evidence

- `cargo fmt --all -- --check` and `git diff --check` passed.
- Focused locked feature check passed with
  `RUST_MIN_STACK=16777216 CARGO_TARGET_DIR=/tmp/glass-165-focused cargo check
  -q -p glass-browser --features native-engine --tests --locked`.
- The focused color resolver unit passed 1/1, the new integration test passed
  1/1, and the complete `current_color` integration subset passed 5/5.
- Full native integration passed 203/203. The feature-enabled browser library
  passed 968 tests with 1 ignored.
- Strict affected-package Clippy passed with all targets/features and
  `-D warnings`; warning-denied feature rustdoc passed; locked `glass-dev`
  check and build passed in the same isolated target.
- Both locked package archives passed with `--no-verify`; the packaged
  dependency checker confirmed `glass-dev` resolves exact `glass-browser =
  0.3.14` without a local path or feature dependency.
- Static gates passed: version sync 0.3.14; feature parity 14 capabilities
  across 4 targets; release docs 579 Markdown/current documents 83/previous
  hits 57/semantic hits 669/current-claim failures 0; TUI 15 implementation
  help keys/63 documentation markers; documentation depth 93 guides/19
  contracts; documentation coverage 579 Markdown, 345 full-product MCP
  tools, 100 browser-only tools, 17 examples, and 22 public modules;
  reliability 6 scenarios/4 targets; adapters 5; Web IR 8 fixtures/8
  scenarios/11 categories; and release-documentation unit tests 9/9.
- Workspace all-target/all-feature testing passed. The feature-enabled
  library run in the workspace passed 969 tests with 1 ignored, native
  integration passed 203/203, `glass-dev` passed 365/365, and all auxiliary
  targets passed.
- Remote CI, push, release, tag, registry publication, browser parity, and
  security-boundary claims remain unmade because the checkout is local-only.

## Cleanup

Before cleanup, the exact disposable inventory was `/tmp/glass-165-focused` at
6,323,920,896 bytes across 9,523 files and 1,202 directories,
`/tmp/glass-165-package` at 3,162,112 bytes across 5 files and 3 directories,
`/tmp/glass-165-workspace` at 4,155,498,496 bytes across 5,541 files and 624
directories, and the release-documentation report at 188,416 bytes. The
workspace test scratch directories had already self-cleaned. No Cargo, rustc,
rustdoc, Clippy, or Glass test writer was active, and `lsof +D` found no open
handles for the exact paths. The repository `target/` directory was absent.
Those exact regenerable paths were removed with bounded same-filesystem
deletion. The measured disposable total was 10,482,769,920 bytes; filesystem
free space rose from 72,132,042,752 to 82,612,920,320 bytes. All exact paths
were verified absent afterward. Shared Cargo registries, toolchains, source,
durable data, and other projects were untouched.
