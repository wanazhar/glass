---
id: native-engine-104
scope: glass-browser/native-engine/text-justify
status: ready
depends-on: [native-engine-103]
---

# Native bounded text-justification control

## Objective

Add an explicit inherited `text-justify` control to the existing fixed-cell
text-flow owner. `none` must suppress separator expansion for both ordinary
soft-wrap justification and explicit final-line justification; `auto` and
`inter-word` retain the current bounded ASCII-space distribution. The slice
must not add a second spacing or geometry owner.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-101.md`
- `docs/plan/tasks/native-engine-102.md`
- `docs/plan/tasks/native-engine-103.md`
- [CSS Text Module Level 3: text-justify](https://www.w3.org/TR/css-text-3/#text-justify-property)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the inherited bounded values
`text-justify:auto|none|inter-word`. The default and unsupported-value
fallback remain `auto`; omitted or unsupported declarations preserve the
current native behavior. `inter-character`, `distribute`, CSS-wide keywords,
and other values remain unsupported and continue to produce the existing typed
diagnostic without raw stylesheet echo.

For eligible horizontal-tb fixed-cell flow:

- `none` disables only the positive `justify_spacing` expansion owned by
  native-engine-101 and native-engine-103. It does not change `text-align`,
  `text-align-last`, line formation, base `word-spacing`, or source/semantic
  order;
- `auto` and `inter-word` are equivalent in this bounded implementation. When
  the applicable alignment is `justify`, eligible collapsed ASCII separators
  continue to receive deterministic integer quotient/remainder shares in
  source order at the existing soft-wrap or final-line flush;
- final-line justification remains controlled by `text-align-last:justify`,
  ordinary non-final justification remains controlled by `text-align:justify`,
  and `text-justify` never turns either alignment value on by itself;
- the existing eligibility gates remain unchanged: `normal|pre-line`
  collapsible whitespace, `word-break:normal`, non-truncated text, emitted
  direct-text fragments with exactly one eligible ASCII space, and positive
  free space. Authored `word-spacing` remains part of base width and is not
  disabled by `text-justify:none`;
- the inherited value is resolved through the existing DOM style walk with
  selector, inline, source-order, and unsupported-value precedence. No
  separator, line, text run, box, or paint command is synthesized when
  `none` removes the extra advance;
- the resolved spacing behavior continues through the existing boxes,
  display list, raster glyphs, viewport projection, overflow, scrolling,
  capture, hit testing, and semantic/source-order consumers.

The slice remains restricted to the current integer-pixel, fixed-cell,
horizontal-tb implementation. Inter-character distribution, language-aware
word breaking, Unicode bidi, shaping, font metrics, `text-justify:inter-word`
browser conformance beyond the ASCII separator model, vertical writing,
logical properties, fractional metrics, hyphenation, and browser-wide CSS
conformance remain outside the contract.

## Tradeoffs

- Reusing the existing boolean justification gates keeps `text-justify:none`
  observable without duplicating line ownership or changing artifact schemas,
  but the model cannot express real word-boundary or glyph-spacing policy.
- Treating `auto` and `inter-word` identically matches the current separator
  algorithm and keeps the default stable, while making the explicit author
  choice testable and leaving inter-character behavior clearly unsupported.
- Applying the control at both soft-wrap and final-line flush prevents a
  surprising partial override; it intentionally does not alter line breaks,
  physical alignment offsets, or authored base spacing.
- No new crate, dependency, renderer, or mutable geometry owner is introduced;
  the native feature stays default-off inside `glass-browser`.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The implementation gate will include:

- parser, declaration, cascade, inheritance, inline precedence, and typed
  unsupported-value diagnostics for `text-justify:auto|none|inter-word`;
- explicit `none` suppression and `auto`/`inter-word` equivalence for ordinary
  soft-wrap and `text-align-last:justify` final-line spacing;
- regressions proving `text-justify` does not enable justification by itself,
  does not change authored `word-spacing`, and does not alter preformatted,
  pre-wrap, break-all, truncated, forced-break, or no-positive-remainder
  behavior;
- shared box/display-list/raster/viewport/overflow/scroll/capture/hit-test and
  semantic/source-order artifact assertions;
- `cargo fmt --all -- --check`, focused and full native integration tests,
  feature-enabled library tests with the documented explicit stack, strict
  all-feature and no-default-feature Clippy, warning-denied rustdoc, locked
  binaries, paired package/dependency gates, fuzz checking,
  documentation/release validators, and exact isolated-target cleanup.

Remote CI remains pending because `main` is local-only. No browser-parity,
release, registry-publication, or remote-certification claim is part of this
task.
