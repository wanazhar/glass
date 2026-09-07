---
id: native-engine-154
scope: glass-browser/native-engine/cascade-border-style-none
status: done
depends-on: [native-engine-153]
---

# Native bounded physical `border-style:none`

## Objective

Extend the bounded physical `border-style` surface with the explicit
case-insensitive `none` style. The `border-style` shorthand expands one to
four `none|solid|dashed|dotted` values into independent physical candidates;
the four physical style longhands own only their side. Each property retains
standalone case-insensitive `revert-layer` support.

## Context

The native engine now resolves complete physical borders and independently
cascades bounded width, color, and `solid|dashed|dotted` style components.
Style-only `none` is still rejected, so a higher layer cannot explicitly turn
off a lower border without coupling to another component. This slice adds an
internal no-paint style sentinel to the existing private style stream. The
sentinel blocks lower style candidates and is converted to no border side
before layout or artifact projection; the public finite paint enum and
display-list schema remain unchanged.

Normative references:

- <https://www.w3.org/TR/css-backgrounds-3/#border-style>
- <https://www.w3.org/TR/css-backgrounds-3/#border-width>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-153.md`
- `docs/plan/tasks/native-engine-019.md`

## Contract

### Declaration and cascade state

- `border-style` accepts one to four values from the bounded
  `none|solid|dashed|dotted` grammar and expands them using the physical
  top/right/bottom/left shorthand mapping. Each physical `border-*-style`
  longhand accepts one style value. All five property names accept one
  standalone, case-insensitive `revert-layer` token.
- Mixed tokens, other CSS-wide keywords, empty values, malformed one-to-four
  value expansions, unsupported styles, and unsupported CSS syntax remain
  typed unsupported-value diagnostics. Raw CSS text is not added to
  diagnostics or public protocol output.
- An explicitly resolved `none` style blocks lower style candidates for that
  side and produces no `NativeBorder` side. A width or color candidate cannot
  resurrect a side whose winning style is `none`; an isolated style-only
  declaration remains non-painting because it still has no width.
- Named-layer priority, specificity, source order, unlayered precedence,
  inline precedence, and same-block declaration order remain authoritative.
  Style `revert-layer` can roll back past an explicit `none`, repeatedly when
  higher layers also roll back, and falls back to no style when no lower style
  candidate remains.
- The no-paint sentinel remains private to CSS cascade resolution. It is
  converted to the existing no-side/zero-width behavior before box-model,
  display-list, capture, raster, point-hit, and semantic/source-order owners.

### Existing owners preserved

- Existing bounded width/style/color component streams continue to resolve
  independently and compose only after all required components resolve.
- Existing `NativeBorderSide`, public `NativeBorderPaintSide`, display-list
  command, rounded mask, clipping, opacity, viewport projection, capture,
  software raster, point-hit, and semantic/source-order schemas remain
  unchanged. No public `NativeBorderStyle::None` variant is added.
- Existing `border` and physical border shorthands retain the current
  complete-value grammar; the `border` shorthand with omitted components,
  `border:none`, and other CSS-wide reset forms remain outside this slice.

The slice remains fixture-relative, horizontal-tb, and bounded to physical
`none|solid|dashed|dotted` styles. It does not add `hidden`, `double`,
`groove`, `ridge`, `inset`, `outset`, logical sides, `currentColor`, gradients,
border-image, animation, multiple origins, `!important` inversion, or
browser-wide CSS border conformance.

## Tradeoffs

- A private no-paint sentinel preserves the public finite paint enum and avoids
  leaking a style that the display-list and rasterizer do not need to replay.
- Converting `none` to no side before layout keeps the existing zero-width and
  no-paint invariants, while the cascade sentinel still blocks lower styles
  correctly.
- Supporting only the style longhands and their bounded shorthand keeps this
  slice small; complete CSS `border:none` grammar and other styles remain
  separately auditable contracts.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- one-to-four-value `none|solid|dashed|dotted` expansion, physical style
  longhands, case-insensitive standalone `revert-layer`, and typed rejection
  of mixed/CSS-wide/malformed/unsupported inputs;
- explicit none blocking lower styles, none rollback to lower paint, repeated
  rollback, named-layer priority, specificity, source order, same-block
  order, unlayered/inline precedence, valid-before-invalid preservation, and
  independent width/color interaction;
- no-side/zero-width geometry, absent border display commands, decoded raster,
  clipping, point-hit testing, capture, and semantic/source order; and
- no public no-paint sentinel leakage or false unsupported diagnostics, plus
  focused `glass-browser` check, targeted behavioral tests, full native
  integration and library tests, strict affected-package Clippy, formatting,
  documentation, and final static gates. Remote CI remains unclaimed until an
  explicitly authorized push.

## Implementation

Implemented in `875cdad8` after the design checkpoint `e7c9ad40`.

- Added a private `NativeBorderStyleValue::{Paint, None}` cascade value so
  explicit `none` can win or be rolled back without adding a public
  `NativeBorderStyle::None` variant.
- Extended the bounded one-to-four-value `border-style` shorthand and all four
  physical style longhands to accept case-insensitive `none` plus standalone
  case-insensitive `revert-layer`.
- Preserved declaration order, named-layer priority, specificity,
  unlayered/inline precedence, invalid-later preservation, and independent
  width/style/color resolution.
- Converts a winning `none` to the existing no-side/zero-width result before
  box-model, display-list, capture, raster, point-hit, and semantic/source-
  order consumers; no public protocol or artifact schema changed.
- Added focused parser/cascade coverage and an artifact integration fixture
  covering blocking, same-layer and repeated rollback, invalid-later
  preservation, physical longhands, mixed components, display commands,
  geometry, raster, point-hit testing, capture, and semantic order.

## Evidence

Local certification passed with the isolated regenerable target
`/tmp/glass-154-focused`:

- `cargo fmt --all -- --check` and `git diff --check` passed.
- Affected `glass-browser` feature check passed.
- Focused border-style unit tests: `3 passed; 965 filtered out`.
- Focused `native_engine` integration test: `1 passed; 191 filtered out`.
- Full native integration: `192 passed; 0 failed`.
- Full native feature library: `967 passed; 1 ignored`.
- Strict affected-package Clippy and feature rustdoc with warnings denied
  passed.
- `glass-dev` check and build passed; both `glass` and `glass-browser` debug
  binaries were produced.
- Final static validators passed: version sync `0.3.14`; feature parity `14`
  capabilities across `4` targets; release documentation `568` Markdown
  documents with `83` current-version documents, `57` previous-version hits,
  `657` semantic-audit hits, and `0` current-claim failures; TUI `15` help
  keys/`63` documentation markers; documentation depth `93` routed guides/
  `19` substantive contracts; reliability `6` scenarios/`4` targets; public
  read-only adapters `5`; Web IR `8` fixtures/`8` scenarios/`11` categories;
  documentation coverage `568` Markdown files/`345` full-product MCP tools
  (`100` browser-only)/`17` examples/`22` public modules.
- The validated target was a real non-symlink directory with no active
  Cargo/Rust consumers or open handles: `5,456,748,544` bytes, `9,191` files,
  and `1,186` directories. It was removed with
  `find -P /tmp/glass-154-focused -xdev -depth -delete`; the target and repo
  `target/` are absent. Available space rose from `78,314,364,928` to
  `83,771,101,184` bytes (`+5,456,736,256` bytes).
- The fresh-binary coverage rerun used a separate real non-symlink target with
  no active consumers or open handles: `/tmp/glass-154-coverage`,
  `2,011,111,424` bytes, `3,282` files, and `509` directories. It was removed
  with the same bounded `find -P … -xdev -depth -delete` procedure; the target
  and repo `target/` remain absent, with final available space
  `83,772,252,160` bytes.
- Remote CI is intentionally unclaimed because this checkout remains local
  and no push was authorized.
