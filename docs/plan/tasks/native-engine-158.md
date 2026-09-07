---
id: native-engine-158
scope: glass-browser/native-engine/cascade-border-hidden-shorthand
status: complete
depends-on: [native-engine-157]
---

# Native bounded omitted-component `border:hidden`

## Objective

Accept the exact case-insensitive omitted-component `border:hidden` form and
the four physical `border-top:hidden`, `border-right:hidden`,
`border-bottom:hidden`, and `border-left:hidden` forms in the native border
owner. Preserve the existing complete `Npx style color` shorthand grammar,
the exact omitted-component `none` support from native-engine-157, and
standalone `revert-layer`.

The new form must enter the existing private hidden style stream so a winning
`hidden` blocks lower styles and cannot be resurrected by width or color
components. The private distinction must survive computed-style resolution so
future collapsed-table conflict work can assign it its own semantics, while
the current non-table engine continues to produce the existing no-side/
zero-width result.

## Context

The native engine already supports physical `border-style:hidden` and the
exact omitted-component `border:none` family. CSS also permits the border
shorthand to omit its other components when the value is exactly `hidden`; the
current parser rejects that syntax even though the private style owner already
has a distinct hidden sentinel. This slice closes that precise shorthand gap
without inventing the full CSS shorthand default machinery (`medium`,
`currentColor`, arbitrary style-only forms, or CSS-wide resets).

Normative reference:

- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthand>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-157.md`
- `docs/plan/tasks/native-engine-155.md`

## Contract

### Declaration and cascade state

- `border`, `border-top`, `border-right`, `border-bottom`, and `border-left`
  accept the existing complete bounded `Npx style color` form, the existing
  standalone case-insensitive `revert-layer` form, the exact
  case-insensitive token `none`, or the exact case-insensitive token `hidden`.
- The exact `hidden` form is represented by a private declaration-only value
  and feeds the existing per-side `NativeBorderStyleValue::Hidden` cascade
  stream. It does not add a public `NativeBorderStyle::Hidden` variant.
- A winning omitted-component `hidden` blocks lower style candidates and
  resolves to the existing no-side/zero-width result in the current non-table
  engine. Width and color candidates cannot resurrect that side. If a later
  style rollback exposes a lower painted style, existing lower component
  candidates remain eligible under the bounded native composition rule.
- The private hidden value remains distinguishable from `none` until the
  current computed-style boundary so future collapsed-table conflict
  resolution can assign hidden its required meaning. This slice does not add
  a table layout or border-conflict owner.
- `hidden` with additional tokens, other CSS-wide keywords, empty values,
  malformed complete values, style-only shorthand values other than exact
  `hidden`/`none`, dimensions without a complete value, and unsupported CSS
  syntax remain typed unsupported-value diagnostics. Raw CSS text is not added
  to diagnostics or public protocol output.
- Named-layer priority, specificity, source order, unlayered precedence,
  inline precedence, same-block declaration order, repeated rollback, and
  valid-before-invalid preservation remain authoritative. A shorthand
  `hidden` participates at its declaration order in all three existing
  component owners, with the private hidden style stream controlling current
  visibility.

### Existing owners preserved

- Complete border declarations retain their current private value and their
  independent width/style/color cascade projections. Omitted-component hidden
  declarations reuse the existing hidden style sentinel rather than changing
  display-list or raster command types.
- `NativeBorderSide`, `NativeBorder`, public `NativeBorderStyle`,
  `NativeBorderPaintSide`, display-list commands, rounded masks, clipping,
  opacity, viewport projection, capture, software raster, point-hit, and
  semantic/source-order schemas remain structurally unchanged.
- The slice remains fixture-relative, horizontal-tb, integer-pixel, and
  non-table. It does not add arbitrary omitted shorthand defaults, CSS-wide
  reset machinery, collapsed-table conflict resolution, logical sides,
  `currentColor`, gradients, border-image, animation, multiple origins,
  `!important` inversion, or browser-wide CSS border conformance.

## Tradeoffs

- Reusing the existing private hidden style sentinel keeps the public paint
  enum and artifact schemas stable, while preserving information needed by a
  future table-conflict owner.
- Exact-only `hidden` closes the high-value shorthand gap without silently
  claiming the default values or full grammar of every omitted-component
  border declaration.
- Omitted-component hidden declarations do not invent width or color
  candidates. This keeps the existing bounded component rollback rule
  explicit and avoids fake geometry or paint values.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- exact case-insensitive `hidden` parsing for the complete and four physical
  border shorthand properties, existing complete values, exact `none`,
  standalone `revert-layer`, and typed rejection of mixed, CSS-wide,
  malformed, style-only, and unsupported inputs;
- private declaration separation, named-layer/specificity/source-order/
  inline precedence, same-block order, repeated rollback, valid-before-invalid
  preservation, and independent width/color interaction;
- hidden-versus-none private separation through current resolution, no-side
  geometry, absent border commands for the current non-table path, decoded
  raster, clipping, point-hit testing, capture, semantic/source order, and
  unchanged public schemas; and
- focused `glass-browser` check/tests, full native integration/library tests,
  strict affected-package Clippy, rustdoc, paired-crate check/build,
  formatting, documentation, and final static gates. Remote CI remains
  unclaimed until an explicitly authorized push.

## Implementation

Implemented in `f04623fc` (`feat(native-engine): support border hidden shorthand`),
with the design contract and synchronized pre-implementation records in
`6041a479`. The existing private `NativeBorderDeclaration` wrapper now carries
`Complete`, `None`, or `Hidden`; exact case-insensitive omitted-component
`hidden` declarations project only into the existing private
`NativeBorderStyleValue::Hidden` stream. Width and color receive no synthetic
candidates, while the hidden distinction remains private for future table
conflict resolution and current non-table composition retains the existing
no-side/zero-width result.

## Evidence

Local certification passed on 2026-09-07 UTC:

- `cargo fmt --all` and `git diff --check` passed.
- Feature-enabled `cargo check -p glass-browser --features native-engine --tests
  --locked` passed with `CARGO_TARGET_DIR=/tmp/glass-158-focused` and
  `RUST_MIN_STACK=16777216`.
- Focused border library selection passed: 17 passed, 951 filtered.
- Focused hidden-shorthand artifact integration test passed: 1 passed, 195
  filtered.
- Full `native_engine` integration suite passed: 196 passed, 0 failed.
- Feature-enabled library suite passed: 967 passed, 1 ignored.
- Strict affected-package Clippy (`--all-targets --all-features -- -D warnings`)
  passed.
- Feature rustdoc with `RUSTDOCFLAGS=-Dwarnings` passed.
- Paired `glass-dev` check/build passed, and both debug binaries were present:
  `glass` and `glass-browser`.
- Static release documentation passed: 572 Markdown documents, 83 current
  documents, 57 previous-version hits, 660 semantic audit hits, and 0 current-
  claim failures. Version sync remained at 0.3.14.
- Static feature parity passed: 14 capabilities across 4 targets. TUI shortcut
  inventory passed at 15 implementation keys and 63 documentation markers;
  documentation depth passed at 93 current guides and 19 substantive
  contracts; reliability passed at 6 scenarios across 4 targets; public
  read-only adapters passed at 5; Web IR passed at 8 fixtures, 8 scenarios,
  and 11 categories; binary documentation coverage passed at 572 Markdown
  files, 345 full-product MCP tools (100 browser-only), 17 examples, and 22
  public modules.

The focused and full integration coverage exercises exact case-insensitive
`hidden` parsing for complete and physical shorthands, mixed/unsupported
rejection, private hidden-versus-none separation, named-layer/specificity/
source-order/inline precedence, same-block order, repeated rollback,
valid-before-invalid preservation, independent width/color interaction,
no-side geometry, border-command presence/absence, decoded raster, point-hit
testing, capture dimensions, and semantic order. No remote CI, push, release,
registry publication, or browser parity claim is made because the checkout
remains local-only.

The regenerable `/tmp/glass-158-focused` target was inventoried as an exact
real directory with no active compiler process or open handle: 5,428,187,136
bytes, 9,070 files, and 1,185 directories. It was removed with bounded
`find -P /tmp/glass-158-focused -xdev -depth -delete`; the path is absent and
free space increased from 77,961,129,984 to 83,389,308,928 bytes. The
repository `target/` directory remains absent.
