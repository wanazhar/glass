---
id: native-engine-164
scope: glass-browser/native-engine/cascade-text-decoration-color-current-color
status: complete
depends-on: [native-engine-163]
---

# Native bounded text-decoration-color `currentColor`

## Objective

Accept the exact case-insensitive `currentColor` keyword for the existing
local `text-decoration-color` property. Resolve it from the element's already
cascaded local or inherited `color` at computed-style construction, then keep
the existing public `Option<NativeColor>`, separate glyph/decoration paint
owners, immutable text command, display-list, and fixed-cell raster contract.

This slice closes the next common paint-color gap without adding a generic CSS
color dependency engine or changing the two-crate architecture.

## Context

The native engine already accepts bounded literal and functional alpha colors
plus standalone `revert-layer` for local `text-decoration-color`. Its private
declaration state distinguishes an explicit decoration color from the omitted
`None` fallback, and the paint path uses that color only for underline,
overline, and line-through pixels. Native-engine-161/162/163 established the
private deferred-color pattern for border and background owners, resolving
`currentColor` after the existing local/inherited `color` owner is selected.

Normative references:

- <https://www.w3.org/TR/css-color-4/#currentcolor-color>
- <https://www.w3.org/TR/css-text-decor-4/#text-decoration-color-property>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-163.md`
- `docs/plan/tasks/native-engine-127.md`

## Contract

### Declaration and cascade state

- `text-decoration-color` accepts every existing bounded literal/alpha color,
  exact case-insensitive `currentColor`, and standalone case-insensitive
  `revert-layer`.
- A private deferred decoration-color value participates in the existing named
  layer, unlayered, inline, specificity, source-order, same-block, and
  valid-before-invalid cascade behavior. It resolves only after local or
  inherited `color` is selected.
- Local `color` wins inherited color; inherited color is used when no local
  color wins; the existing bounded black fallback is used when neither exists.
  `text-decoration-color` remains local/non-inherited: a descendant without a
  winning declaration retains the existing omitted-color fallback rather than
  inheriting a parent's explicit decoration color.
- An explicit `currentColor` resolves to the current element color, while an
  omitted decoration color remains `None` and continues to use the existing
  glyph-color fallback in paint replay. Transparent/literal values and
  `revert-layer` rollback preserve their existing semantics.
- Malformed values, other CSS-wide keywords, gradients, image functions,
  system colors, color-space functions, percentages, arbitrary functions, and
  unresolved declaration keywords remain typed unsupported-value diagnostics and
  do not replace an earlier valid declaration. The local `color` property
  remains concrete-only.

### Existing owners preserved

- `NativeTextDecorationColorDeclaration::CurrentColor` remains private until
  computed-style resolution. Public computed style, `NativeDisplayCommand`,
  capture bytes, raster schemas, dependencies, feature defaults, and crate
  boundaries remain unchanged.
- The resolved optional decoration color continues through the existing
  separate glyph/decoration text command fields, decoration geometry, clipping,
  opacity, scrolling, capture, software raster, point-hit, and
  semantic/source-order consumers. Glyph pixels keep resolved text color.
- The slice remains fixture-relative, horizontal-tb, integer-pixel,
  fixed-cell, local-property, non-table, and feature-gated behind
  `native-engine` inside `glass-browser`.

## Tradeoffs

- Adding one private deferred variant to the existing decoration-color enum is
  smaller and safer than exposing a keyword or introducing a generic CSS value
  graph. It duplicates the proven resolve branch while keeping property owners
  readable.
- Resolving at computed-style construction makes all downstream text artifacts
  consume one concrete color and avoids resolving `currentColor` separately in
  display-list or raster code. It does not attempt custom properties, cycles,
  animations, or multi-origin precedence.
- The explicit current-color path is tested beside literal, transparent,
  omitted, and rollback cases so the distinction between explicit decoration
  color and the existing omitted fallback stays observable.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, product docs, and
  issue records

## Verification

The completed slice must cover:

- case-insensitive parser acceptance for literal, transparent,
  `currentColor`, and `revert-layer`, plus typed rejection of unsupported and
  unrelated forms;
- local, inherited, and bounded-black substitution; local-property
  non-inheritance; layer/unlayered/inline precedence; specificity;
  same-block order; valid-before-invalid preservation; and private/public
  separation;
- separate glyph and decoration colors, decoration patterns, clipping,
  opacity, capture/raster pixels, point-hit, semantic/source order, and
  unchanged omitted-color fallback; and
- focused feature check/tests, full native integration/library tests, strict
  affected-package Clippy, rustdoc, paired-crate check/build, packaging,
  formatting, documentation, final static gates, workspace all-target/
  all-feature testing, and bounded regenerable-target cleanup. Remote CI
  remains unclaimed until an explicitly authorized push.

## Implementation

Implemented in `cceb61bf` from design checkpoint `e11821c5`; the diagnostics
fixture follow-up is `005083c3` and synchronized product documentation is
`2960ecc5`.

- Added a private `CurrentColor` variant to the local decoration-color
  declaration owner, preserving the public `Option<NativeColor>` computed
  style and the immutable text command/display/raster schemas.
- Accepted exact case-insensitive `text-decoration-color: currentColor`,
  resolved it after the existing local/inherited `color` owner is selected,
  and retained the bounded black fallback when neither color is present.
- Preserved local-property non-inheritance, named-layer/unlayered/inline
  precedence, specificity, source order, same-block order, valid-before-
  invalid handling, transparent values, `revert-layer`, and omitted-color
  fallback behavior.
- Added an integration fixture covering local, inherited, black-fallback,
  rollback, invalid-later, inline, transparent, omitted, opacity, geometry,
  hit-test, semantic/source order, display-list, PNG raster, and typed
  diagnostics. The diagnostics fixture now uses an unsupported gradient so it
  continues to test typed rejection after `currentColor` became supported.

## Evidence

- `cargo fmt --all` and `git diff --check` passed.
- Focused feature check passed:
  `RUST_MIN_STACK=16777216 CARGO_TARGET_DIR=/tmp/glass-164-focused cargo check -q -p glass-browser --features native-engine --tests --locked`.
- Focused decoration-color unit coverage passed 3/3 and the focused integration
  fixture passed 1/1.
- Full native integration passed 202/202. Feature-enabled library tests
  passed 968, with 1 ignored.
- Strict affected-package Clippy passed with all targets/features and
  `-D warnings`; warning-denied feature rustdoc passed; paired locked
  `glass-dev` check and build passed.
- Both locked package archives were created with `--no-verify`; the packaged
  dependency checker confirmed `glass-dev` resolves exact `glass-browser =
  0.3.14` without a local path or feature dependency.
- Static gates passed: version sync 0.3.14; feature parity 14 capabilities
  across 4 targets; release docs 578 Markdown/current documents 83/previous
  hits 57/semantic hits 668/current-claim failures 0; TUI 15 implementation
  help keys/63 documentation markers; documentation depth 93 guides/19
  contracts; documentation coverage 578 Markdown, 345 full-product MCP
  tools, 100 browser-only tools, 17 examples, and 22 public modules;
  reliability 6 scenarios/4 targets; adapters 5; Web IR 8 fixtures/8
  scenarios/11 categories; and release-documentation unit tests 9/9.
- Workspace all-target/all-feature testing passed. The matrix included
  feature-enabled `glass-browser` with 969 passed and 1 ignored library test,
  202/202 native integration tests, `glass-dev` with 365/365 tests, and all
  auxiliary targets.
- Before cleanup, `/tmp/glass-164-focused` contained 6,764,908,544 measured
  bytes (9,887 files/1,204 directories), `/tmp/glass-164-package` contained
  3,162,112 bytes (5 files/3 directories), and `/tmp/glass-164-workspace`
  contained 4,154,884,096 bytes (5,541 files/624 directories). No cargo,
  rustc, rustdoc, or clippy process or open handle targeted those paths.
  Exact bounded deletion removed 10,922,954,752 measured bytes; filesystem
  free space rose from 71,870,935,040 to 82,793,873,408 bytes. All three
  temporary paths were verified absent afterward.
- Remote CI, push, release, tag, registry publication, browser parity, and
  security-boundary claims remain unmade because this checkout is local-only.
