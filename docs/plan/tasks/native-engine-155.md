---
id: native-engine-155
scope: glass-browser/native-engine/cascade-border-style-hidden
status: done
depends-on: [native-engine-154]
---

# Native bounded physical `border-style:hidden`

## Objective

Extend the bounded physical `border-style` surface with the explicit
case-insensitive `hidden` style. The `border-style` shorthand expands one to
four `none|hidden|solid|dashed|dotted` values into independent physical
candidates; the four physical style longhands own only their side. Each
property retains standalone case-insensitive `revert-layer` support.

## Context

The native engine now resolves explicit physical `none` through a private
no-paint sentinel. CSS defines `hidden` as no paint with the same zero used
border width as `none`, while reserving a distinction for collapsed-table
border conflict resolution. This slice keeps that distinction private even
though the current engine has no table-conflict owner.

Normative reference:

- <https://www.w3.org/TR/css-backgrounds-3/#border-style>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-154.md`
- `docs/plan/tasks/native-engine-019.md`

## Contract

### Declaration and cascade state

- `border-style` accepts one to four values from the bounded
  `none|hidden|solid|dashed|dotted` grammar and expands them using the
  physical top/right/bottom/left shorthand mapping. Each physical
  `border-*-style` longhand accepts one style value. All five property names
  accept one standalone, case-insensitive `revert-layer` token.
- Mixed tokens, other CSS-wide keywords, empty values, malformed one-to-four
  value expansions, unsupported styles, and unsupported CSS syntax remain
  typed unsupported-value diagnostics. Raw CSS text is not added to
  diagnostics or public protocol output.
- An explicitly resolved `hidden` style blocks lower style candidates and
  produces no `NativeBorder` side. In the current non-table engine it has the
  same no-side/zero-width result as `none`; the private distinction is retained
  for a future collapsed-table conflict owner. Width or color candidates cannot
  resurrect a side whose winning style is `hidden`; an isolated style-only
  declaration remains non-painting because it still has no width.
- Named-layer priority, specificity, source order, unlayered precedence,
  inline precedence, and same-block declaration order remain authoritative.
  Style `revert-layer` can roll back past an explicit `hidden`, repeatedly when
  higher layers also roll back, and falls back to no style when no lower style
  candidate remains.
- The private `hidden` sentinel remains inside CSS cascade resolution. It is
  converted to the existing no-side/zero-width behavior before box-model,
  display-list, capture, raster, point-hit, and semantic/source-order owners.

### Existing owners preserved

- Existing bounded width/style/color component streams continue to resolve
  independently and compose only after all required components resolve.
- Existing `NativeBorderSide`, public `NativeBorderStyle`, public
  `NativeBorderPaintSide`, display-list command, rounded mask, clipping,
  opacity, viewport projection, capture, software raster, point-hit, and
  semantic/source-order schemas remain unchanged. No public `hidden` variant
  is added and no table-conflict semantics are claimed.
- Existing `border` and physical border shorthands retain the current
  complete-value grammar; `border: hidden`, omitted-component `border:none`,
  and other CSS-wide reset forms remain outside this slice.

The slice remains fixture-relative, horizontal-tb, and bounded to physical
`none|hidden|solid|dashed|dotted` styles. It does not add collapsed-table
border conflict resolution, `double`, `groove`, `ridge`, `inset`, `outset`,
logical sides, `currentColor`, gradients, border-image, animation, multiple
origins, `!important` inversion, or browser-wide CSS border conformance.

## Tradeoffs

- A distinct private hidden sentinel preserves future collapsed-table meaning
  without leaking a public style enum variant or changing current artifacts.
- Treating hidden as no-side/zero-width at the existing composition boundary
  matches current CSS used-value behavior and avoids inventing a table owner
  before the engine has table layout or conflict resolution.
- Supporting only the existing physical style stream keeps this slice small;
  complete border shorthand grammar and table behavior remain separately
  auditable contracts.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- one-to-four-value `none|hidden|solid|dashed|dotted` expansion, physical style
  longhands, case-insensitive standalone `revert-layer`, and typed rejection
  of mixed/CSS-wide/malformed/unsupported inputs;
- explicit hidden blocking lower styles, hidden rollback to lower paint,
  repeated rollback, named-layer priority, specificity, source order,
  same-block order, unlayered/inline precedence, valid-before-invalid
  preservation, and independent width/color interaction;
- no-side/zero-width geometry, absent border display commands, decoded raster,
  clipping, point-hit testing, capture, and semantic/source order; and
- no public hidden sentinel leakage or false unsupported diagnostics, plus
  focused `glass-browser` check, targeted behavioral tests, full native
  integration and library tests, strict affected-package Clippy, formatting,
  documentation, and final static gates. Remote CI remains unclaimed until an
  explicitly authorized push.

## Implementation

Implemented in `2b07f109` after the design checkpoint `37126fa1`.

- Added a distinct private `NativeBorderStyleValue::Hidden` cascade value;
  it cannot leak into the public `NativeBorderStyle` enum or display-list
  schema.
- Extended bounded case-insensitive `border-style` shorthand and all four
  physical style longhands to accept `hidden`, while retaining standalone
  case-insensitive `revert-layer` and the existing `none|solid|dashed|dotted`
  behavior.
- Preserved named-layer priority, specificity, source order, unlayered/inline
  precedence, same-block declaration order, valid-before-invalid preservation,
  and independent width/style/color resolution.
- Treats a winning `hidden` as no-side/zero-width before box-model and artifact
  projection in the current non-table engine, while retaining the private
  distinction for future collapsed-table conflict resolution.
- Added parser and artifact integration coverage for shorthand expansion,
  physical longhands, rollback, precedence, invalid-later preservation,
  mixed-component blocking, geometry, display commands, raster, hit testing,
  semantic order, and PNG capture.

## Evidence

Local certification passed with the isolated regenerable target
`/tmp/glass-155-focused`:

- `cargo fmt --all -- --check` and `git diff --check` passed.
- Affected `glass-browser` feature check passed.
- Focused border-style unit tests: `3 passed; 965 filtered out`.
- Focused hidden-border integration test: `1 passed; 192 filtered out`.
- Full native integration: `193 passed; 0 failed`.
- Full native feature library: `967 passed; 1 ignored`.
- Strict affected-package Clippy and feature rustdoc with warnings denied
  passed.
- `glass-dev` check and build passed; both `glass` and `glass-browser` debug
  binaries were produced.
- Final static validators passed: version sync `0.3.14`; feature parity `14`
  capabilities across `4` targets; release documentation `569` Markdown
  documents with `83` current-version documents, `57` previous-version hits,
  `657` semantic-audit hits, and `0` current-claim failures; TUI `15` help
  keys/`63` documentation markers; documentation depth `93` routed guides/
  `19` substantive contracts; reliability `6` scenarios/`4` targets; public
  read-only adapters `5`; Web IR `8` fixtures/`8` scenarios/`11` categories;
  documentation coverage `569` Markdown files/`345` full-product MCP tools
  (`100` browser-only)/`17` examples/`22` public modules.
- The validated target was a real non-symlink directory with no active
  Cargo/Rust consumers or open handles: `5,459,587,072` bytes, `9,199` files,
  and `1,186` directories. It was removed with
  `find -P /tmp/glass-155-focused -xdev -depth -delete`; the target and repo
  `target/` are absent. Available space rose from `78,319,292,416` to
  `83,778,842,624` bytes (`+5,459,550,208` bytes).
- Remote CI is intentionally unclaimed because this checkout remains local
  and no push was authorized.
