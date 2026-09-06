---
id: native-engine-138
scope: glass-browser/native-engine/cascade-layers-gap-revert-layer
status: complete
depends-on: [native-engine-137]
---

# Native bounded gap-family `revert-layer`

## Objective

Reuse the bounded cascade-layer machinery proven by native-engine-137 to add
standalone, case-insensitive `revert-layer` to the existing finite `gap`,
`row-gap`, and `column-gap` declarations. The shorthand and longhands must
resolve each physical gap component independently while preserving the
already-certified flex item/line geometry and all downstream artifacts.

## Context

The native engine already parses finite non-negative integer-pixel `gap`,
`row-gap`, and `column-gap` values and expands the shorthand into its row and
column components. The current cascade helper keeps shorthand/longhand source
order, selector specificity, unlayered precedence, and inline precedence in
one concrete candidate. This slice changes only the private declaration state
needed to roll back a winning component to lower bounded candidates.

Normative references:

- <https://www.w3.org/TR/css-align-3/#gap-properties>
- <https://www.w3.org/TR/css-flexbox-1/#gap-properties>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-137.md`

## Contract

### Declaration and cascade state

- `gap: revert-layer`, `row-gap: revert-layer`, and
  `column-gap: revert-layer` accept one standalone, case-insensitive token.
  They are represented only by private component declaration state; the
  public computed style remains `u32` values and no new artifact field is
  introduced.
- `gap: revert-layer` writes rollback candidates to both the row and column
  components. `row-gap` and `column-gap` write rollback to only their matching
  component.
- A later valid longhand in the same declaration block overrides only its
  component; a later valid `gap` shorthand resets both components. Invalid
  declarations remain ignored and do not erase an earlier valid declaration.
- The existing bounded first-appearance named-layer order remains authoritative.
  Specificity and source order decide ties inside a layer; unlayered and inline
  declarations remain above named layers.
- A winning rollback blocks only its current bounded layer for that physical
  component and resolves through lower concrete shorthand/longhand candidates
  or the native fallback of `0`.
- The gap family remains non-inherited. Descendants without local candidates
  retain a zero local fallback regardless of an ancestor's gap.
- Existing finite one- and two-value `gap` forms and finite longhands remain
  concrete component values, including established shorthand/longhand source
  order and row/column flex-axis mapping.
- Other CSS-wide keywords, multiple origins, `!important` inversion, layer
  statements, nested/anonymous/comma layers, percentages, negative or
  fractional lengths, grid conformance, and browser-wide gap conformance
  remain outside the contract.

### Existing owners preserved

Resolved row and column gaps continue through row/column main-axis placement,
wrapped-line formation, cross-line distribution, overflow and root-scroll
projection, point hit testing, display-list generation, software
rasterization, and semantic/source-order owners. No public computed-style or
layout/display-list schema, dependency, feature default, or crate boundary
changes are permitted.

The slice remains a horizontal-tb, fixed-cell, bounded local-content contract.
It changes only which existing gap component candidates win the cascade; it
does not add intrinsic or percentage sizing, grid behavior, or browser parity.

## Implementation and local result

- Design checkpoint: `656dc37d` (`docs(native-engine): plan gap rollback`).
- Implementation checkpoint: `bded96ae` (`feat(native-engine): add gap
  rollback`).
- Added private declaration-aware gap parsers and four fixed bounded
  row/column cascade candidate arrays; the public computed gap values remain
  finite `u32` pixels.
- `gap:revert-layer` rolls back both physical components while the two
  longhands roll back independently; same-block shorthand/longhand order,
  named-layer priority, unlayered/inline precedence, invalid-declaration
  handling, zero fallback, and non-inherited descendant behavior are covered.
- Existing layout, overflow/scroll, point-hit, display-list, software-raster,
  semantic, and source-order owners consume the resolved values without public
  schema, dependency, feature, or crate-boundary changes.
- Focused native test binary: 7 gap parser/cascade tests passed.
- Focused integration: 1 gap row/column layout and artifact test passed.
- Full native integration: 175 tests passed.
- Full `glass-browser` native-feature library: 940 tests passed, 1 ignored,
  with `RUST_MIN_STACK=33554432`.
- Strict affected-package Clippy (`glass-browser`, native-engine,
  all-targets, locked, `-D warnings`) passed.
- `cargo fmt --all` and `git diff --check` passed.
- Static version, release-documentation, documentation-depth/coverage,
  feature-parity, TUI-shortcut, adapter, reliability, and Web IR gates passed
  for the synchronized checkout: 552 Markdown documents, 83 current guides,
  649 semantic audit hits, zero current-claim failures, 345 full-product MCP
  tools, 17 examples, 22 public modules, 14 parity capabilities across four
  targets, 15 implementation help keys, 63 documentation markers, 5
  read-only adapters, 6 reliability scenarios across four targets, and 8 Web
  IR fixtures across 11 categories.
- The bounded `/tmp/glass-138-focused` target measured 6,597,239,094 logical
  bytes across 6,985 files and 892 directories after all Cargo and static
  gates; it had no open handles and was removed with bounded `find -P
  /tmp/glass-138-focused -xdev -depth -delete`. `/tmp` free space increased
  from 77,372,166,144 to 83,989,454,848 bytes, reclaiming 6,617,288,704
  bytes, and the target path is absent.
- Remote CI, push, release, tag, registry publication, and browser parity are
  not claimed; this checkout remains local-only.

## Tradeoffs

- Expanding the shorthand into two private component candidates avoids a public
  shorthand state and makes rollback semantics match the already-proven
  component resolver, at the cost of four small fixed candidate arrays in the
  style walk.
- Keeping the existing `GapCascadeValue` precedence tuple preserves current
  same-block behavior, but resolution must explicitly merge shorthand and
  longhand candidates before applying layer rollback.
- Retaining finite integer-pixel parsing keeps compilation and runtime bounded,
  while CSS-wide semantics beyond `revert-layer` and percentage resolution
  remain intentionally typed as unsupported.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The focused gate must cover:

- standalone case-insensitive parsing and typed rejection of other CSS-wide,
  mixed, malformed, and finite forms;
- named-layer priority, repeated rollback, unlayered/inline precedence,
  independent row/column fallback, and same-block shorthand/longhand order;
- row and column placement, wrapping, line distribution, overflow/scroll, and
  source/semantic order through the existing layout owner;
- hit testing, display-list geometry, and decoded-raster coordinates; and
- no false unsupported-value diagnostics or public declaration-keyword
  leakage.

Targeted checks follow the completed gap behavioral unit. Full native,
two-crate, strict, fuzz/security, and static documentation gates are final
validation only; remote CI remains unclaimed until an explicitly authorized
push.
