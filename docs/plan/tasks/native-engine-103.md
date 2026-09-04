---
id: native-engine-103
scope: glass-browser/native-engine/text-align-last-justify
status: complete
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

## Implementation and verification evidence

The design checkpoint is `f261773f` (`docs(native-engine): define final-line
justification slice`) and the implementation checkpoint is `be5757ae`
(`feat(native-engine): support final-line justification`). The implementation
keeps the feature inside `glass-browser`, adds no crate or dependency, and
routes final-line spacing through the existing layout, display-list, raster,
viewport, overflow, capture, hit-test, and semantic consumers.

The completed local gate evidence is:

- focused final-line artifact coverage: 1/1;
- focused parser/cascade coverage: 2/2 `text_align_last` tests;
- unsupported-value diagnostic regression: 1/1;
- full native integration suite: 140/140;
- feature-enabled library suite with `RUST_MIN_STACK=8388608`: 893 passed,
  1 ignored, 0 failed;
- strict all-feature Clippy: passed in 14m03s; strict no-default-feature
  Clippy: passed in 6m09s;
- warning-denied workspace rustdoc: passed in 3m16s;
- locked `glass-dev` binaries: passed in 11m43s;
- locked paired packages: both crates packaged successfully and
  `check-packaged-dependency.py` confirmed `glass-dev` resolves
  `glass-browser` exactly at 0.3.14; Cargo emitted only the known yanked
  `chacha20 v0.10.1` warning;
- locked fuzz fetch and offline all-target check: passed in 8m22s;
- version sync, feature parity, release-documentation, TUI shortcut,
  documentation-depth, documentation-coverage, reliability, public-adapter,
  and Web IR validators all passed: 517 Markdown documents, 83 current
  documents, 57 previous-version hits, 585 semantic hits, 0 current-claim
  failures; 15 implementation help keys/63 documentation markers; 93/19
  depth; 517/345/17/22 coverage; 6/4 reliability; 5 adapters; and 8/8/11
  Web IR fixtures/scenarios/categories;
- `cargo fmt --all -- --check` and `git diff --check` passed.

The exact isolated cleanup gate ran after validation. `/tmp/glass-103-target`
measured 6.2G and the generated release report measured 164K. Process and
open-file checks were empty; the exact target, report, and 103 log files were
removed with bounded `find -P ... -xdev -depth -delete`. No `/tmp/glass-103*`,
`glass-clean-install.*`, or other temporary `glass-*-target` paths remain.
The repository and ForgeBuild target directories remain only as 4.0K
placeholders, the fuzz target is absent, and the final filesystem state is
65G available at 67% use.

Remote CI remains pending because `main` is local-only. No browser-parity,
release, registry-publication, or remote-certification claim is part of this
task.
