---
id: native-engine-163
scope: glass-browser/native-engine/cascade-background-current-color
status: complete
depends-on: [native-engine-162]
---

# Native bounded background `currentColor`

## Objective

Accept the exact case-insensitive `currentColor` keyword for the existing
`background-color` property. Resolve it from the element's already-cascaded
local or inherited `color` at computed-style construction, then preserve the
existing concrete public `Option<NativeColor>` and fill/display/raster owners.

This slice does not add `color: currentColor`, gradients, images, system
colors, color spaces, percentages, CSS-wide reset machinery, or any new crate,
dependency, feature default, public schema, or backend boundary.

## Context

The native engine already resolves explicit `currentColor` for physical border
color declarations in private `NativeBorderColorValue` state and substitutes
the computed local/inherited color before projecting public border values.
`background-color` still uses the concrete-only local color parser, even though
its computed style is resolved after the inherited color owner is available.
This leaves a common fill use case inconsistent with the certified border path.

Normative references:

- <https://www.w3.org/TR/css-color-4/#currentcolor-color>
- <https://www.w3.org/TR/css-backgrounds-3/#background-color>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-162.md`
- `docs/plan/tasks/native-engine-161.md`

## Contract

### Declaration and cascade state

- `background-color` accepts every existing bounded literal color plus exact,
  case-insensitive `currentColor` and the existing standalone
  `revert-layer` declaration.
- A private background deferred-color value participates in the existing
  named-layer, unlayered, inline, specificity, source-order, same-block, and
  valid-before-invalid cascade rules. It resolves only after local/inherited
  `color` has been selected.
- An element-local color wins over inherited color; an inherited color is used
  when no local color wins; the existing bounded black fallback is used when
  neither exists. `background-color: currentColor` is not inherited itself.
- `revert-layer`, transparent/literal colors, invalid later declarations,
  no-candidate `None`, and existing layer fallbacks retain their current
  behavior.
- Malformed values, other CSS-wide keywords, gradients, image functions,
  system colors, color-space functions, percentages, and arbitrary functions
  remain typed unsupported-value diagnostics and do not replace an earlier
  valid declaration.

### Existing owners preserved

- Deferred background color remains private until computed-style resolution.
  `NativeComputedStyle::background_color` remains `Option<NativeColor>`;
  `NativeDisplayCommand::FillRect`, layout boxes, clipping, opacity groups,
  viewport projection, capture, software raster, point-hit, and semantic/source
  order remain unchanged.
- The local `color` property remains concrete-only. This slice does not infer a
  self-referential `color: currentColor` value or alter inherited text color.
- The slice remains fixture-relative, horizontal-tb, integer-pixel, non-table,
  and feature-gated behind `native-engine` inside `glass-browser`.

## Tradeoffs

- A separate private background deferred-color type keeps the border-specific
  cascade state readable and avoids widening any public color representation;
  the small duplication is preferable to coupling unrelated property owners.
- Resolving at the computed-style boundary reuses the already-certified color
  owner and makes fill, display-list, capture, and raster consumers see one
  concrete value. It does not attempt browser-wide custom-property or color
  dependency resolution.
- Covering literal, transparent, `currentColor`, and rollback behavior in one
  bounded property slice prevents a partial fill contract while keeping the
  unsupported grammar explicit.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, product docs, and
  issue records

## Verification

The completed slice must cover:

- case-insensitive parser acceptance for literal and `currentColor` values,
  `revert-layer`, invalid-later preservation, and typed rejection of
  unsupported/background-unrelated forms;
- local, inherited, and bounded-black current-color substitution, with
  declaration order, specificity, layer/unlayered/inline precedence, and
  independent local `color` behavior;
- `None`/transparent behavior, fill display-list color, layout, clipping,
  opacity/capture/raster output, point-hit, semantic/source order, and private
  versus public value separation; and
- focused feature check/tests, full native integration/library tests, strict
  affected-package Clippy, rustdoc, paired-crate check/build, formatting,
  documentation, final static gates, and bounded regenerable-target cleanup.
  Remote CI remains unclaimed until an explicitly authorized push.

## Implementation

Implemented in `7a406855` from design checkpoint `764e0c86`.

- Added a private `NativeBackgroundColorValue` deferred-color owner for
  `background-color`, preserving the public `Option<NativeColor>` computed-style
  field and existing fill/display/raster schemas.
- Accepted exact case-insensitive `currentColor` alongside existing literal
  colors and standalone `revert-layer`; resolved it after local/inherited
  `color` selection with the existing bounded black fallback.
- Kept the local `color` parser concrete-only and preserved invalid-later,
  layer/unlayered/inline, specificity, source-order, transparency, and
  no-candidate behavior.
- Added an integration fixture covering local, inherited, black-fallback,
  rollback, invalid-later, inline, transparent, no-background, opacity,
  geometry, hit-test, semantic order, display-list, PNG raster, and typed
  diagnostics.

## Evidence

- `cargo fmt --all -- --check` and `git diff --check` passed.
- Focused feature check passed:
  `RUST_MIN_STACK=16777216 CARGO_TARGET_DIR=/tmp/glass-163-focused cargo check -q -p glass-browser --features native-engine --tests --locked`.
- Focused parser unit passed: 1/1. Focused background integration passed: 1/1.
- Full native integration passed: 201/201. Feature-enabled library tests
  passed: 968 passed, 1 ignored.
- Strict affected-package Clippy passed with all targets/features and
  `-D warnings`; feature rustdoc passed with warnings denied.
- Paired locked `glass-dev` check and build passed.
- Both locked package archives were created with `--no-verify`; the packaged
  dependency checker confirmed `glass-dev` resolves exact `glass-browser
  =0.3.14` without a local path or feature dependency.
- Static gates passed: version sync 0.3.14; feature parity 14 capabilities
  across 4 targets; release docs 577 Markdown/current-claim failures 0;
  TUI 15 implementation keys/63 documentation markers; documentation depth
  93 guides/19 contracts; reliability 6 scenarios/4 targets; adapters 5;
  Web IR 8 fixtures/8 scenarios/11 categories; documentation coverage 577
  Markdown, 345 full-product MCP tools, 17 examples, and 22 public modules.
- Workspace all-target/all-feature testing passed with
  `RUST_MIN_STACK=16777216 CARGO_TARGET_DIR=/tmp/glass-163-workspace cargo test -q --workspace --all-targets --all-features --locked`;
  the native integration target passed 201/201 within that matrix.
- Before cleanup, `/tmp/glass-163-focused` contained 5,403,737,325 bytes
  (9,074 files/1,185 directories), `/tmp/glass-163-package` contained
  3,128,163 bytes (5 files/3 directories), and `/tmp/glass-163-workspace`
  contained 4,140,113,351 bytes (5,541 files/624 directories). No cargo,
  rustc, rustdoc, or clippy process or open handle targeted those paths.
  Exact bounded deletion reclaimed 9,546,978,839 measured bytes; filesystem
  free space rose from 73,223,565,312 to 82,814,054,400 bytes. All three
  temporary paths were verified absent afterward.
- Remote CI, push, release, tag, registry publication, browser parity, and
  security-boundary claims remain unmade because this checkout is local-only.
