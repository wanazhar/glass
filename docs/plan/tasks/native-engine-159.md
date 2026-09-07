---
id: native-engine-159
scope: glass-browser/native-engine/cascade-border-hidden-complete
status: complete
depends-on: [native-engine-158]
---

# Native bounded complete `border: Npx hidden color`

## Objective

Accept the bounded complete-value `Npx hidden color` form for `border`,
`border-top`, `border-right`, `border-bottom`, and `border-left`, with
case-insensitive `hidden`. Preserve the exact omitted-component `none` and
`hidden` forms from native-engine-157/158, the complete painted `Npx style
color` grammar, and standalone `revert-layer`.

The complete hidden form must carry its declared width and color into the
existing independent component streams while mapping only its style to the
private `NativeBorderStyleValue::Hidden` sentinel. A winning hidden style must
still suppress current non-table paint, but its width/color state must remain
available to the bounded computed-style owner and future collapsed-table
conflict work. No public `Hidden` paint variant or new display-list schema is
permitted.

## Context

The native engine now accepts exact omitted-component `border:hidden` and the
four physical omitted-component forms, but its complete parser accepts only
the painted public styles. CSS also permits `hidden` as the style component of
a complete border value, for example `2px hidden red`. The current parser
rejects that syntax because `NativeBorderSide` intentionally stores only
public painted styles. This slice adds a private complete-hidden value rather
than leaking the table-sensitive state into public computed values.

Normative reference:

- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthand>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-158.md`
- `docs/plan/tasks/native-engine-157.md`

## Contract

### Declaration and cascade state

- The five physical border shorthand properties accept the existing complete
  bounded `Npx style color` form for public painted styles, the exact
  case-insensitive omitted-component `none` or `hidden` token, the complete
  bounded `Npx hidden color` form, or standalone case-insensitive
  `revert-layer`.
- A complete hidden declaration is represented by a private declaration-only
  value carrying its bounded width and parsed color. Its style projection is
  `NativeBorderStyleValue::Hidden`; its width and color projections are the
  declared values at the same declaration order. It does not add a public
  `NativeBorderStyle::Hidden` variant.
- A winning complete hidden style blocks current non-table paint and resolves
  to the existing no-side/zero-width result. Width and color remain typed
  private candidates for the future collapsed-table owner, but they cannot
  resurrect a painted side in the current computed-style composition. A later
  bounded `revert-layer` can expose an existing lower painted component.
- Complete hidden values with missing width/color, extra tokens, unsupported
  styles, other CSS-wide keywords, empty values, malformed dimensions/colors,
  style-only values other than the exact omitted-component token, and
  unsupported CSS syntax remain typed unsupported-value diagnostics. Raw CSS
  text is not added to diagnostics or public protocol output.
- Named-layer priority, specificity, source order, unlayered precedence,
  inline precedence, same-block declaration order, repeated rollback, and
  valid-before-invalid preservation remain authoritative for all three
  component streams.

### Existing owners preserved

- Exact omitted-component `hidden` continues to use the 158 private hidden
  declaration and does not invent width/color candidates. Complete painted
  borders and exact omitted-component `none` remain unchanged.
- `NativeBorderSide`, `NativeBorder`, public `NativeBorderStyle`,
  `NativeBorderPaintSide`, display-list commands, rounded masks, clipping,
  opacity, viewport projection, capture, software raster, point-hit, and
  semantic/source-order schemas remain structurally unchanged.
- The slice remains fixture-relative, horizontal-tb, integer-pixel, and
  non-table. It does not add collapsed-table conflict resolution, logical
  sides, arbitrary omitted defaults, CSS-wide reset machinery, `currentColor`,
  gradients, border-image, animation, multiple origins, `!important`
  inversion, or browser-wide CSS border conformance.

## Tradeoffs

- A private complete-hidden value preserves declared width/color for future
  table conflict resolution without forcing a public enum expansion or
  pretending that current non-table rendering paints hidden borders.
- The parser remains exact and bounded: it adds `Npx hidden color`, not
  arbitrary omitted combinations, CSS-wide keyword semantics, or a general
  hidden-border layout model.
- Width/color candidates are retained internally even though current
  composition suppresses them behind hidden style. This is deliberate state
  ownership for future table work and must be covered by private/public
  separation tests.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- case-insensitive complete `Npx hidden color` parsing for the shorthand and
  all four physical properties, preservation of complete painted/omitted
  values, standalone `revert-layer`, and typed rejection of incomplete,
  mixed, malformed, CSS-wide, and unsupported inputs;
- private complete-hidden width/style/color separation, declaration order,
  named-layer/specificity/source-order/inline precedence, same-block order,
  repeated rollback, and valid-before-invalid preservation;
- current no-side/zero-width geometry and absent border commands/raster while
  declared width/color remain privately typed, plus clipping, point-hit,
  capture, semantic/source order, and unchanged public schemas; and
- focused `glass-browser` check/tests, full native integration/library tests,
  strict affected-package Clippy, rustdoc, paired-crate check/build,
  formatting, documentation, and final static gates. Remote CI remains
  unclaimed until an explicitly authorized push.

## Implementation

Implemented in `f04623fc` (`feat(native-engine): support border hidden shorthand`),
with the design contract and synchronized pre-implementation records in
`bdbdb208`. The private `NativeBorderDeclaration` wrapper now carries a
complete hidden value with its declared width and color. Complete hidden
projects width and color into their independent private candidates while
projecting hidden style to `NativeBorderStyleValue::Hidden`; current non-table
composition suppresses paint without changing public computed or artifact
schemas. Exact omitted-component `none`/`hidden` and complete painted values
remain unchanged.

## Evidence

Local certification passed on 2026-09-07 UTC:

- `cargo fmt --all` and `git diff --check` passed.
- Feature-enabled `cargo check -p glass-browser --features native-engine --tests
  --locked` passed with `CARGO_TARGET_DIR=/tmp/glass-159-focused` and
  `RUST_MIN_STACK=16777216`.
- Focused border library selection passed: 17 passed, 951 filtered.
- Focused complete-hidden artifact integration test passed: 1 passed, 196
  filtered.
- Full `native_engine` integration suite passed: 197 passed, 0 failed.
- Feature-enabled library suite passed: 967 passed, 1 ignored.
- Strict affected-package Clippy (`--all-targets --all-features -- -D warnings`)
  passed.
- Feature rustdoc with `RUSTDOCFLAGS=-Dwarnings` passed.
- Paired `glass-dev` check/build passed, and both debug binaries were present:
  `glass` and `glass-browser`.
- Static release documentation passed: 573 Markdown documents, 83 current
  documents, 57 previous-version hits, 662 semantic audit hits, and 0 current-
  claim failures. Version sync remained at 0.3.14.
- Static feature parity passed: 14 capabilities across 4 targets. TUI shortcut
  inventory passed at 15 implementation keys and 63 documentation markers;
  documentation depth passed at 93 current guides and 19 substantive
  contracts; reliability passed at 6 scenarios across 4 targets; public
  read-only adapters passed at 5; Web IR passed at 8 fixtures, 8 scenarios,
  and 11 categories; binary documentation coverage passed at 573 Markdown
  files, 345 full-product MCP tools (100 browser-only), 17 examples, and 22
  public modules.

The focused and full integration coverage exercises complete and physical
case-insensitive `Npx hidden color`, exact omitted-component hidden/none,
complete painted values, mixed/incomplete/malformed rejection, private
hidden-versus-painted separation, declaration order, named-layer/specificity/
source-order/inline precedence, same-block style override, repeated rollback,
valid-before-invalid preservation, retained width/color behavior, current
no-side geometry and raster suppression, reopened painted artifacts, point-hit
testing, capture dimensions, and semantic order. No remote CI, push, release,
registry publication, or browser parity claim is made because the checkout
remains local-only.

The regenerable `/tmp/glass-159-focused` target was inventoried as an exact
real directory with no active compiler process or open handle: 5,666,385,920
bytes, 9,099 files, and 1,192 directories. It was removed with bounded
`find -P /tmp/glass-159-focused -xdev -depth -delete`; the path is absent and
free space increased from 77,711,831,040 to 83,378,364,416 bytes. The
repository `target/` directory remains absent.
