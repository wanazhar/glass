---
id: native-engine-157
scope: glass-browser/native-engine/cascade-border-none-shorthand
status: complete
depends-on: [native-engine-156]
---

# Native bounded omitted-component `border:none`

## Objective

Accept the exact case-insensitive omitted-component `border:none` form and
the four physical `border-top:none`, `border-right:none`, `border-bottom:none`,
and `border-left:none` forms in the native border owner. Preserve the existing
complete `Npx style color` shorthand grammar and standalone `revert-layer`.
The new form must enter the existing private no-paint style stream so a
winning `none` blocks lower styles and cannot be resurrected by width or color
components.

## Context

The native engine already supports physical `border-style:none` and the
complete-value `border`/physical border shorthands. CSS also permits a
border shorthand to omit its other components when the value is exactly
`none`; the current parser rejects that syntax, leaving a stale product claim
that the bounded no-paint style is available only through `border-style`.
This slice adds the smallest useful omitted-component form without inventing
the full CSS shorthand default machinery (`medium`, `currentColor`, arbitrary
style-only forms, or CSS-wide resets).

Normative reference:

- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthand>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-156.md`
- `docs/plan/tasks/native-engine-154.md`

## Contract

### Declaration and cascade state

- `border`, `border-top`, `border-right`, `border-bottom`, and `border-left`
  accept the existing complete bounded `Npx style color` form, the existing
  standalone case-insensitive `revert-layer` form, or the exact
  case-insensitive token `none`.
- The exact `none` form is represented by a private declaration-only
  no-paint value and feeds the existing per-side `NativeBorderStyleValue::None`
  cascade stream. It does not add a public `NativeBorderStyle::None` variant.
- A winning omitted-component `none` blocks lower style candidates and
  resolves to the existing no-side/zero-width result. Width and color
  candidates cannot resurrect that side. If a later style rollback exposes a
  lower painted style, existing lower component candidates remain eligible;
  this is the bounded native component-composition rule, not a claim of full
  CSS shorthand-default parity.
- `none` with additional tokens, other CSS-wide keywords, empty values,
  malformed complete values, `hidden` shorthand, style-only shorthand forms,
  dimensions without a complete value, and unsupported CSS syntax remain typed
  unsupported-value diagnostics. Raw CSS text is not added to diagnostics or
  public protocol output.
- Named-layer priority, specificity, source order, unlayered precedence,
  inline precedence, same-block declaration order, repeated rollback, and
  valid-before-invalid preservation remain authoritative. A shorthand `none`
  participates at its declaration order in all three existing component
  owners, with the no-paint style stream controlling current visibility.

### Existing owners preserved

- Complete border declarations retain their current private value and their
  independent width/style/color cascade projections. Physical no-paint
  shorthand declarations reuse the existing style sentinel rather than
  changing display-list or raster command types.
- `NativeBorderSide`, `NativeBorder`, public `NativeBorderStyle`,
  `NativeBorderPaintSide`, display-list commands, rounded masks, clipping,
  opacity, viewport projection, capture, software raster, point-hit, and
  semantic/source-order schemas remain structurally unchanged.
- The slice remains fixture-relative, horizontal-tb, integer-pixel, and
  non-table. It does not add `border:hidden` shorthand, arbitrary omitted
  shorthand defaults, CSS-wide reset machinery, collapsed-table conflict
  resolution, logical sides, `currentColor`, gradients, border-image,
  animation, multiple origins, `!important` inversion, or browser-wide CSS
  border conformance.

## Tradeoffs

- A private declaration wrapper is a small cascade-shape change, but it keeps
  complete borders and no-paint shorthand values typed instead of encoding
  `none` through a fake width, color, or public enum value.
- The slice accepts only exact `none`; this closes the high-value no-paint
  syntax gap without silently claiming `medium`, `currentColor`, or every
  omitted-component combination.
- No-paint shorthand declarations do not invent width or color candidates.
  This preserves existing lower-component rollback behavior and keeps the
  current engine’s explicit style-stream ownership visible to reviewers.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- exact case-insensitive `none` parsing for the complete and four physical
  border shorthand properties, existing complete values, standalone
  `revert-layer`, and typed rejection of mixed, CSS-wide, malformed,
  `hidden`, and unsupported inputs;
- private declaration separation, named-layer/specificity/source-order/
  inline precedence, same-block order, repeated rollback, valid-before-invalid
  preservation, and independent width/color interaction;
- no-side geometry, absent border commands, decoded raster, clipping,
  point-hit testing, capture, semantic/source order, and unchanged public
  schemas; and
- focused `glass-browser` check/tests, full native integration/library tests,
  strict affected-package Clippy, rustdoc, paired-crate check/build,
  formatting, documentation, and final static gates. Remote CI remains
  unclaimed until an explicitly authorized push.

## Implementation

Implemented in `fb2c56a2` (`feat(native-engine): support border none shorthand`),
with design contract and synchronized pre-implementation records in
`ab9d6628`. A private `NativeBorderDeclaration::Complete|None` wrapper now
preserves complete border values while allowing exact case-insensitive
omitted-component `none` declarations to participate in the existing per-side
cascade. `none` projects only into the private no-paint style stream; width
and color do not receive synthetic candidates, and the public computed,
display-list, raster, capture, hit-test, and semantic schemas are unchanged.

## Evidence

Local certification passed on 2026-09-07 UTC:

- `cargo fmt --all` and `git diff --check` passed.
- Feature-enabled `cargo check -p glass-browser --features native-engine --tests
  --locked` passed with `CARGO_TARGET_DIR=/tmp/glass-157-focused` and
  `RUST_MIN_STACK=16777216`.
- Focused border library selection passed: 17 passed, 951 filtered.
- Focused artifact integration test passed: 1 passed, 194 filtered.
- Full `native_engine` integration suite passed: 195 passed, 0 failed.
- Feature-enabled library suite passed: 967 passed, 1 ignored.
- Strict affected-package Clippy (`--all-targets --all-features -- -D warnings`)
  passed.
- Feature rustdoc with `RUSTDOCFLAGS=-Dwarnings` passed.
- Paired `glass-dev` check/build passed, and both debug binaries were present:
  `glass` and `glass-browser`.
- Static release documentation passed: 571 Markdown documents, 83 current
  documents, 57 previous-version hits, 659 semantic audit hits, and 0 current-
  claim failures. Version sync remained at 0.3.14.
- Static feature parity passed: 14 capabilities across 4 targets. TUI shortcut
  inventory passed at 15 implementation keys and 63 documentation markers;
  documentation depth passed at 93 current guides and 19 substantive
  contracts; reliability passed at 6 scenarios across 4 targets; public
  read-only adapters passed at 5; Web IR passed at 8 fixtures, 8 scenarios,
  and 11 categories; binary documentation coverage passed at 571 Markdown
  files, 345 full-product MCP tools (100 browser-only), 17 examples, and 22
  public modules.

The focused test covers complete and physical exact `none`, mixed-token and
unsupported rejection, private/public separation, layer/source-order and
inline precedence, repeated rollback, valid-before-invalid preservation,
independent width/color interaction, no-side geometry, border-command
presence/absence, decoded raster, point hit testing, capture dimensions, and
semantic order. No remote CI, push, release, registry publication, or browser
parity claim is made because the checkout remains local-only.

The regenerable `/tmp/glass-157-focused` target was inventoried as an exact
real directory with no active compiler process or open handles: 5,462,691,840
bytes, 9,198 files, and 1,186 directories. It was removed with bounded
`find -P /tmp/glass-157-focused -xdev -depth -delete`; the path is absent and
free space increased from 78,323,441,664 to 83,786,129,408 bytes. The
repository `target/` directory remains absent.
