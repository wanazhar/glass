---
id: native-engine-151
scope: glass-browser/native-engine/cascade-border-color
status: done
depends-on: [native-engine-150]
---

# Native bounded physical `border-color`

## Objective

Add standalone, case-insensitive `border-color`, `border-top-color`,
`border-right-color`, `border-bottom-color`, and `border-left-color` to the
existing physical border owner. The shorthand expands one to four values into
independent physical color candidates; longhands own only their side. Each
property also accepts one standalone `revert-layer` token and resolves through
the existing bounded 15-layer registry without leaking a declaration keyword
into `NativeBorder` or the display list.

## Context

`native-engine-150` gave the existing `border` and physical side shorthands
private per-side candidate stacks for width, style, and color as one bounded
border value. The native engine already has a typed color grammar, deterministic
solid/dashed/dotted border replay, physical box-model insets, rounded masks,
capture, raster, point-hit, and semantic/source-order projections. The missing
surface is the common color-only border shorthand and physical longhands.

This slice adds a second private color component stream. Existing `border`
declarations contribute their color to that stream so a color-only declaration
can override or roll back the color component without changing width or style.
The parser records declaration positions for the border component streams so
same-block shorthand/longhand order is retained across the existing rule-level
cascade key.

Normative references:

- <https://www.w3.org/TR/css-backgrounds-3/#border-color>
- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthands>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-150.md`
- `docs/plan/tasks/native-engine-014.md`
- `docs/plan/tasks/native-engine-018.md`

## Contract

### Declaration and cascade state

- `border-color` accepts one to four values from the existing bounded
  `NativeColor` grammar and expands them using the physical top/right/bottom/
  left shorthand mapping. Each physical `border-*-color` longhand accepts one
  existing color value. Every five property names accept one standalone,
  case-insensitive `revert-layer` token.
- Mixed tokens, other CSS-wide keywords, empty values, malformed one-to-four
  value expansions, unsupported color syntax, and values rejected by the
  existing color parser remain typed unsupported-value diagnostics. Raw color
  text is not added to diagnostics or public protocol output.
- Existing `border` and `border-top|right|bottom|left` declarations continue
  to own width and style. Their selected color also participates in the private
  color candidate stream. A color-only declaration changes only color; it does
  not invent width/style or paint a zero-width border.
- Named-layer priority, specificity, source order, unlayered precedence,
  inline precedence, and same-block declaration order remain authoritative.
  A color `revert-layer` blocks only its current bounded layer in the color
  stream, can roll back repeatedly, and falls back to the bounded initial
  black color when no lower color candidate remains. Width/style rollback and
  color rollback remain independent component decisions.
- The resolved color stream is combined with the resolved physical border
  width/style stream only after both have resolved. `revert-layer` never enters
  the public `NativeBorder` value.

### Existing owners preserved

- Resolved colors continue through the existing `NativeBorderSide`, outer and
  content box insets, physical border display-list command, rounded mask,
  ancestor clips, opacity groups, viewport projection, point-hit testing,
  capture dimensions, software raster, and semantic/source-order projection.
- Existing border width, style, pattern phase, side order, corner precedence,
  and zero-width/no-paint behavior remain unchanged.
- No public computed-style field, display-list command, raster schema,
  diagnostic transport, dependency, feature default, or crate boundary
  changes.

The slice remains fixture-relative, horizontal-tb, and bounded to the current
physical color grammar. It does not add standalone border-width/style streams,
logical sides, `currentColor`, gradients, border-image, system colors,
animation, multiple origins, `!important` inversion, or browser-wide CSS
border conformance.

## Tradeoffs

- A private color stream makes color-only overrides useful without duplicating
  the border geometry or paint owners, while keeping component rollback
  explicit.
- Carrying existing shorthand colors into the stream preserves the current
  border behavior and makes `revert-layer` fall back to the lower complete
  border declaration when no color-only declaration exists.
- Same-block declaration-position metadata adds a small amount of parser state
  and avoids a fixed application order silently changing `border` versus
  `border-color`; it does not attempt to generalize declaration ordering to all
  CSS properties.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- one-to-four-value `border-color` expansion, physical longhands, existing
  color grammar, case-insensitive standalone `revert-layer`, and typed
  rejection of mixed/CSS-wide/malformed/unsupported inputs;
- independent width/style versus color ownership, named-layer priority,
  specificity, source order, same-block shorthand/longhand order, repeated
  rollback, unlayered/inline precedence, valid-before-invalid preservation,
  and the bounded black color fallback;
- physical border geometry, display-list color, capture, decoded raster,
  clipping, point-hit testing, and semantic/source order; and
- no false unsupported diagnostics or public rollback leakage, plus focused
  `glass-browser` check, targeted behavioral tests, full native integration and
  library tests, strict affected-package Clippy, formatting, documentation,
  and final static gates. Remote CI remains unclaimed until an explicitly
  authorized push.

## Implementation

Implemented in `c26482b7`. The parser now accepts the five bounded physical
`border-color` names, expands valid one-to-four-value shorthands, preserves
case-insensitive standalone `revert-layer`, and rejects mixed/CSS-wide/
malformed forms through the existing typed diagnostic path. `NativeDeclarations`
retains per-side declaration positions for both existing border values and the
new color values. Computed style resolves an independent per-side color
candidate stream, carries selected colors into the existing `NativeBorderSide`,
and leaves width/style cascade, display-list, raster, capture, hit, and
semantic/source-order owners unchanged.

## Evidence

Local certification completed in the isolated regenerable target
`/tmp/glass-151-focused`:

- `CARGO_TARGET_DIR=/tmp/glass-151-focused cargo check -q -p glass-browser --features native-engine --tests --locked` passed.
- Focused parser/cascade units: 2 passed, 961 filtered.
- Focused integration: 1 passed, 188 filtered.
- Full native integration: 189 passed, 0 failed.
- Affected library with `RUST_MIN_STACK=16777216`: 962 passed, 1 ignored.
- Strict affected-package Clippy with `-D warnings` passed.
- Feature rustdoc with `RUSTDOCFLAGS=-Dwarnings` passed.
- `CARGO_TARGET_DIR=/tmp/glass-151-focused cargo check -q -p glass-dev --locked` and
  `CARGO_TARGET_DIR=/tmp/glass-151-focused cargo build -q -p glass-dev --locked`
  passed.
- `cargo fmt --all`, `git diff --check`, and the focused artifact assertions
  passed; no public rollback keyword or unsupported diagnostic leaked from the
  feature surface.
- Final static documentation, coverage, and bounded-target cleanup evidence is
  recorded below after the documentation checkpoint.
