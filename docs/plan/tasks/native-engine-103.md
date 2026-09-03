---
id: native-engine-103
scope: glass-browser/native-engine/text-align-last-justify
status: ready
depends-on: [native-engine-102]
---

# Native bounded final-line justification

## Objective

Extend the existing bounded `text-align-last` owner with the explicit
`justify` value. A final non-empty line that reaches the owning block's normal
completion flush may distribute positive free space across eligible collapsed
ASCII separators, reusing the integer spacing and artifact path established by
native-engine-101 and the final-line boundary established by
native-engine-102.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-101.md`
- `docs/plan/tasks/native-engine-102.md`
- [CSS Text Module Level 3: text-align-last](https://www.w3.org/TR/css-text-3/#text-align-last-property)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the inherited bounded values
`text-align-last:auto|left|center|right|start|end|justify`. The established
fallback remains `auto`; omitted or unsupported declarations retain the
current native line behavior. `match-parent`, `justify-all`, and other values
remain unsupported and continue to produce the existing typed diagnostic.

For eligible horizontal-tb fixed-cell block flow:

- `text-align-last:justify` applies only to the final non-empty line flushed by
  the owning block's normal completion path. It may justify the final line
  after an earlier explicit `<br>` because the forced-break flush itself stays
  ordinary and the trailing line is still completed by the final flush;
- final-line justification requires the existing `normal|pre-line`
  collapsible-whitespace mode, `word-break:normal`, non-truncated text, and
  emitted direct-text fragments containing exactly one eligible ASCII space;
  positive free space is distributed with the same deterministic integer
  quotient/remainder shares and source order used by 101;
- `text-align` remains independent: final-line `justify` works even when the
  ordinary alignment is `left`, while ordinary soft-wrap justification remains
  owned by `text-align:justify` and 101;
- authored `word-spacing` remains part of the base separator width and the
  final-line `justify_spacing` remains a separate additive advance. No
  separator, line, text run, box, or paint command is synthesized when there
  is no eligible space or no positive remainder;
- `auto`, physical, and direction-aware logical final-line values retain the
  102 behavior. Preformatted/pre-wrap chunks, break-all flow, ellipsis or
  truncated runs, inline-item preflight, intermediate block boundaries, and
  the forced-break line before `<br>` remain outside final-line justification;
- the resolved offset and separator advance continue through the existing
  boxes, display list, raster glyphs, viewport projection, overflow,
  scrolling, capture, hit testing, and semantic/source-order consumers;
- the property remains inherited through the existing DOM style walk, and
  selector, inline, source-order, and unsupported-value precedence remains
  typed and bounded.

The slice remains restricted to the current horizontal-tb, integer-pixel,
fixed-cell implementation. Unicode bidi resolution, glyph shaping, mixed
direction runs, language-specific line breaking, inter-character justification,
`text-justify`, logical properties, vertical writing modes, fractional or
font-relative metrics, hyphenation, and browser-wide CSS conformance remain
outside the contract.

## Tradeoffs

- Reusing the 101 spacing fields and 102 final flush keeps final-line
  justification observable through every existing artifact consumer without a
  second geometry owner, but it intentionally supports only the established
  fixed-cell ASCII separator model.
- Making `justify` explicit avoids changing the 102 `auto` fallback and keeps
  ordinary `text-align:justify` behavior limited to non-final soft-wrapped
  lines; authors must opt into final-line distribution.
- Keeping forced-break and preformatted paths ordinary avoids inventing line
  ownership for paths whose whitespace semantics are not yet modeled, while
  permitting the final line after a break to use the already-defined final
  flush.
- No new crate, dependency, renderer, or mutable geometry owner is
  introduced; the native feature stays default-off inside `glass-browser`.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The implementation gate will include:

- parser, declaration, cascade, inheritance, inline precedence, and typed
  unsupported-value diagnostics for `text-align-last:justify`;
- final-line justification with ordinary left alignment, inherited and
  overridden values, positive/non-positive free space, authored word spacing,
  no-text/no-space fallback, and a final line after an explicit `<br>`;
- regressions proving 101 soft-wrap justification remains limited to eligible
  non-final lines and that preformatted, pre-wrap, break-all, truncated, and
  intermediate/forced-break flushes do not gain final-line spacing;
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
