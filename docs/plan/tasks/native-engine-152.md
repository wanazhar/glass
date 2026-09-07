---
id: native-engine-152
scope: glass-browser/native-engine/cascade-border-width
status: done
depends-on: [native-engine-151]
---

# Native bounded physical `border-width`

## Objective

Add standalone, case-insensitive `border-width`, `border-top-width`,
`border-right-width`, `border-bottom-width`, and `border-left-width` to the
existing physical border owner. The shorthand expands one to four bounded
integer-pixel values into independent physical width candidates; longhands
own only their side. Each property also accepts one standalone,
case-insensitive `revert-layer` token.

## Context

The native engine currently accepts bounded physical `border` and side
shorthands, physical `border-color` and side color longhands, and private
per-side cascade streams for border values and colors. Border width is still
only carried by the complete `border` value, so a width-only declaration
cannot override or roll back width without coupling to style and color. This
slice adds a private width component stream and combines it with the existing
resolved border style and color only at computed-style composition.

Normative references:

- <https://www.w3.org/TR/css-backgrounds-3/#border-width>
- <https://www.w3.org/TR/css-backgrounds-3/#border-shorthands>
- <https://www.w3.org/TR/css-cascade-5/#cascade-layer-revert>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-151.md`
- `docs/plan/tasks/native-engine-018.md`
- `docs/plan/tasks/native-engine-019.md`

## Contract

### Declaration and cascade state

- `border-width` accepts one to four values from the existing bounded
  non-negative integer-pixel dimension grammar and expands them using the
  physical top/right/bottom/left shorthand mapping. Each physical
  `border-*-width` longhand accepts one existing dimension value. All five
  property names accept one standalone, case-insensitive `revert-layer` token.
- Mixed tokens, other CSS-wide keywords, empty values, malformed one-to-four
  value expansions, percentages, negative or fractional dimensions, and
  unsupported units remain typed unsupported-value diagnostics. Raw CSS text is
  not added to diagnostics or public protocol output.
- Existing `border` and `border-top|right|bottom|left` declarations contribute
  their selected widths to the private width stream. A width-only declaration
  changes only width; it cannot invent a border style or color and therefore
  cannot create a new `NativeBorder` or paint a zero-style border.
- Named-layer priority, specificity, source order, unlayered precedence,
  inline precedence, and same-block declaration order remain authoritative.
  Width `revert-layer` blocks only its current bounded layer, can roll back
  repeatedly, and falls back to bounded zero width when no lower width
  candidate remains. Style and color candidate decisions remain independent.
- The resolved width stream is combined with the existing resolved border
  style stream and the 151 color stream only after each component resolves.
  `revert-layer` never enters the public `NativeBorder` value.

### Existing owners preserved

- Resolved widths continue through the existing `NativeBorderSide`, outer and
  content box insets, physical border display-list command, rounded mask,
  ancestor clips, opacity groups, viewport projection, point-hit testing,
  capture dimensions, software raster, and semantic/source-order projection.
- Existing border style, color, side order, pattern phase, corner precedence,
  and zero-width/no-paint behavior remain unchanged. A width-only declaration
  does not make a style-bearing border appear.
- No public computed-style field, display-list command, raster schema,
  diagnostic transport, dependency, feature default, or crate boundary
  changes.

The slice remains fixture-relative, horizontal-tb, and bounded to the current
physical integer-pixel dimension grammar. It does not add standalone
`border-style`, logical sides, `currentColor`, gradients, border-image,
fractional or percentage widths, animation, multiple origins,
`!important` inversion, or browser-wide CSS border conformance.

## Tradeoffs

- A private width stream makes width-only overrides useful without duplicating
  style/color or geometry owners, while keeping component rollback explicit.
- Carrying existing `border` widths into the stream preserves current border
  behavior and lets a width rollback expose a lower complete border width.
- A bounded zero-width fallback avoids inventing CSS-wide initial metrics when
  no lower native width candidate exists; the existing style/color values may
  remain computed but cannot paint through a zero-width side.
- Same-block declaration-position metadata is reused from the color slice and
  only generalizes ordering for this width component, not for all CSS.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan, README, and issue
  records

## Verification

The completed slice must cover:

- one-to-four-value `border-width` expansion, physical width longhands,
  existing dimension grammar, case-insensitive standalone `revert-layer`, and
  typed rejection of mixed/CSS-wide/malformed/unsupported inputs;
- independent width versus style/color ownership, named-layer priority,
  specificity, source order, same-block shorthand/longhand order, repeated
  rollback, unlayered/inline precedence, valid-before-invalid preservation,
  and bounded zero-width fallback;
- physical border geometry, display-list width, capture, decoded raster,
  clipping, point-hit testing, and semantic/source order; and
- no false unsupported diagnostics or public rollback leakage, plus focused
  `glass-browser` check, targeted behavioral tests, full native integration and
  library tests, strict affected-package Clippy, formatting, documentation,
  and final static gates. Remote CI remains unclaimed until an explicitly
  authorized push.

## Implementation

Implemented in `7dfcc7f5`. The parser accepts the five bounded physical
`border-width` names, expands valid one-to-four-value shorthands, preserves
case-insensitive standalone `revert-layer`, and rejects mixed/CSS-wide/
malformed forms through the existing typed diagnostic path. `NativeDeclarations`
retains per-side declaration positions for the existing border values and new
width values. Computed style resolves an independent per-side width candidate
stream, carries selected widths into the existing `NativeBorderSide`, and
leaves style/color cascade, display-list, raster, capture, hit, and
semantic/source-order owners unchanged.

## Evidence

Local certification completed in the isolated regenerable target
`/tmp/glass-152-focused`:

- `CARGO_TARGET_DIR=/tmp/glass-152-focused cargo check -q -p glass-browser --features native-engine --tests --locked` passed.
- Focused parser/cascade units: 2 passed, 963 filtered.
- Focused integration: 1 passed, 189 filtered.
- Full native integration: 190 passed, 0 failed.
- Affected library with `RUST_MIN_STACK=16777216`: 964 passed, 1 ignored.
- Strict affected-package Clippy with `-D warnings` passed.
- Feature rustdoc with `RUSTDOCFLAGS=-Dwarnings` passed.
- `CARGO_TARGET_DIR=/tmp/glass-152-focused cargo check -q -p glass-dev --locked` and
  `CARGO_TARGET_DIR=/tmp/glass-152-focused cargo build -q -p glass-dev --locked`
  passed.
- `cargo fmt --all`, `git diff --check`, and the focused artifact assertions
  passed; width-only declarations remain non-painting without a style and no
  rollback keyword or unsupported diagnostic leaked from the feature surface.
- Final static gates passed: 567 Markdown documents; 83 current-version
  documents; 57 previous-version hits; 657 semantic audit hits; 0 current-claim
  failures; feature parity 14 capabilities across 4 targets; TUI 15
  implementation help keys and 63 documentation markers; documentation depth
  93 guides and 19 substantive contracts; reliability 6 scenarios across 4
  targets; public read-only adapters 5; Web IR 8 fixtures, 8 scenarios, and
  11 categories; and documentation coverage 567 Markdown files, 345 full-
  product MCP tools (100 browser-only), 17 examples, and 22 public modules.

## Cleanup evidence

- After all implementation, test, documentation, and coverage gates passed,
  `/tmp/glass-152-focused` was verified as a real directory with no active
  Cargo/Rust consumer and no open handles, then removed with bounded,
  same-filesystem `find -P /tmp/glass-152-focused -xdev -depth -delete`.
- The target contained 5,373,452,288 bytes, 8,886 files, and 1,131
  directories. Available `/tmp` space increased from 78,405,451,776 bytes to
  83,778,924,544 bytes, and `/home/ubuntu/work/glass/target` remains absent.
