---
id: native-engine-104
scope: glass-browser/native-engine/text-justify
status: complete
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

## Implementation and verification

Implementation checkpoint: `d83b24e4`. The CSS cascade and DOM style walk now
carry inherited `text-justify:auto|none|inter-word`; the existing layout owner
gates separator expansion for both soft-wrap and explicit final-line flushes.
`auto` and `inter-word` preserve the existing bounded ASCII-space algorithm,
while `none` removes only the positive justification advance. No new crate,
dependency, artifact schema, geometry owner, or browser-wide conformance claim
was introduced.

Local evidence:

- parser/declaration/cascade/inheritance tests: 2/2;
- focused text-justify integration: 1/1; unsupported-diagnostic regression:
  1/1; full native integration: 141/141;
- feature-enabled `glass-browser` library: 895 passed, 1 ignored, 0 failed
  with `RUST_MIN_STACK=8388608`;
- strict all-feature workspace Clippy passed in 13m21s; strict no-default-
  feature `glass-browser` Clippy in 5m53s; warning-denied workspace rustdoc in
  3m10s; locked `glass-dev --bins` compilation in 11m19s;
- locked paired packages passed; the packaged dependency validator confirmed
  `glass-dev` resolves exactly to `glass-browser` `0.3.14`. Cargo emitted the
  known non-fatal yanked `chacha20 v0.10.1` lockfile warning;
- locked fuzz fetch and offline all-target checking passed in 8m29s;
- version, feature-parity, release-documentation, TUI-shortcut,
  documentation-depth, documentation-coverage, reliability, public-adapter,
  and Web IR validators passed: release docs 518 Markdown documents / 83
  current / 57 previous-version hits / 588 semantic hits / 0 current-claim
  failures; coverage 518/345/17/22; TUI 15/63; depth 93/19; reliability 6/4;
  adapters 5; Web IR 8/8/11;
- `cargo fmt --all -- --check` and `git diff --check` passed.

Remote CI remains pending because `main` is local-only. No browser-parity,
release, registry-publication, or remote-certification claim is part of this
task.

## Cleanup

After process and open-file checks found no active consumers, the exact
regenerable `/tmp/glass-104-target` measured 6.2G and the temporary reports
and logs were removed. No `/tmp/glass-*-target` directories remain; the
project and ForgeBuild targets remain 4.0K each; `fuzz/target` is absent; and
the filesystem reports 65G available at 67% use.

Remote CI remains pending because `main` is local-only. No browser-parity,
release, registry-publication, or remote-certification claim is part of this
task.
