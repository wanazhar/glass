---
id: native-engine-102
scope: glass-browser/native-engine/text-align-last
status: complete
depends-on: [native-engine-101]
---

# Native bounded final-line alignment

## Objective

Add a bounded inherited `text-align-last` property to the native engine's
existing fixed-cell inline-flow owner. Resolve an explicit final-line value
only at the final non-empty line flush of a block, while preserving the 101
soft-wrap justification boundary, source order, and the single geometry path.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-101.md`
- [CSS Text Module Level 3: text-align-last](https://www.w3.org/TR/css-text-3/#text-align-last-property)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the inherited bounded values
`text-align-last:auto|left|center|right|start|end`. The established fallback
remains `auto`; omitted or unsupported declarations retain the current native
line behavior. `justify`, `match-parent`, `justify-all`, and other values are
unsupported and continue to produce the existing typed diagnostic.

For eligible horizontal-tb fixed-cell block flow:

- only the final non-empty line flushed by the owning block's normal completion
  path uses `text-align-last`; soft-wrap, inline-item preflight, explicit
  `<br>`, source-newline, preformatted chunk, break-all, and intermediate
  block-boundary flushes continue using the ordinary `text-align` owner;
- `auto` preserves the current `text-align` result except that
  `text-align:justify` keeps its 101 final-line fallback to physical left
  alignment and does not stretch separators;
- explicit `left|center|right` uses physical alignment, while `start|end`
  resolves through the inherited `direction:ltr|rtl` state already used by
  native-engine-100; no text or semantic/source order is reordered;
- a final empty line is not aligned and does not create a synthetic text run,
  box, or paint command;
- the resolved offset is applied by the existing line-flush owner to every
  line item and text run in the final line. The same geometry reaches boxes,
  display-list commands, raster glyphs, viewport projection, overflow,
  scrolling, capture, hit testing, and semantic/source-order consumers;
- the property remains inherited through the existing DOM style walk, and
  selector, inline, source-order, and unsupported-value precedence remains
  typed and bounded.

The slice remains restricted to the current horizontal-tb, integer-pixel,
fixed-cell implementation. Unicode bidi resolution, glyph shaping, mixed
direction runs, `text-align-last:justify`, `text-align:match-parent`,
`text-align:justify-all`, `text-justify`, logical properties, vertical writing
modes, fractional or font-relative metrics, hyphenation, and browser-wide CSS
conformance remain outside the contract.

## Tradeoffs

- Reusing the final normal block flush keeps final-line alignment numerically
  consistent across all existing artifact consumers, but does not claim
  forced-break and last-line behavior from the full CSS text model.
- Keeping `auto` as a distinct value preserves the native compatibility
  fallback for `text-align:justify` without rewriting authored declarations;
  explicit values opt into final-line placement.
- Limiting the property to the final non-empty block line avoids inventing
  line ownership for nested or forced-break paths, but it leaves those paths
  on ordinary `text-align` until a separate contract supplies evidence.
- No new crate, dependency, renderer, or mutable geometry owner is introduced;
  the native feature stays default-off inside `glass-browser`.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Implementation and verification evidence

The design checkpoint is `fc396200` (`docs(native-engine): define final-line
alignment slice`) and the implementation checkpoint is `1157bf49`
(`feat(native-engine): support final-line text alignment`). The implementation
keeps the feature inside `glass-browser`, adds no crate or dependency, and
routes final-line offsets through the existing layout, display-list, raster,
viewport, overflow, capture, hit-test, and semantic consumers.

The completed local gate evidence is:

- focused parser/cascade coverage: 2/2 `text_align_last` tests;
- focused final-line artifact geometry: 1/1;
- unsupported-value diagnostic regression: 1/1;
- full native integration suite: 139/139;
- feature-enabled library suite with `RUST_MIN_STACK=8388608`: 893 passed,
  1 ignored, 0 failed;
- strict all-feature Clippy: passed in 13m30s; strict no-default-feature
  Clippy: passed in 6m15s;
- warning-denied workspace rustdoc: passed in 3m14s;
- locked `glass-dev` binaries: passed in 11m27s;
- locked paired packages: both crates packaged successfully and
  `check-packaged-dependency.py` confirmed `glass-dev` resolves
  `glass-browser` exactly at 0.3.14; Cargo emitted only the known yanked
  `chacha20 v0.10.1` warning;
- locked fuzz fetch and offline all-target check: passed in 8m38s;
- version sync, feature parity, release-documentation, TUI shortcut,
  documentation-depth, documentation-coverage, reliability, public-adapter,
  and Web IR validators all passed: 516 Markdown documents, 83 current
  documents, 57 previous-version hits, 582 semantic hits, 0 current-claim
  failures; 15 implementation help keys/63 documentation markers; 93/19
  depth; 516/345/17/22 coverage; 6/4 reliability; 5 adapters; and 8/8/11
  Web IR fixtures/scenarios/categories;
- `cargo fmt --all -- --check` and `git diff --check` passed.

The exact isolated cleanup gate ran after validation. `/tmp/glass-102-target`
measured 6.2G and the generated release report measured 160K. Process and
open-file checks were empty; the exact target, report, and 102 log files were
removed with bounded `find -P ... -xdev -depth -delete`. No `/tmp/glass-102*`
or `glass-clean-install.*` paths remain, the repository and fuzz targets were
not retained, and the final filesystem state is 65G available at 67% use.

Remote CI remains pending because `main` is local-only. No browser-parity,
release, registry-publication, or remote-certification claim is part of this
task.
