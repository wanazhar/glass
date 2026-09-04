---
id: native-engine-105
scope: glass-browser/native-engine/text-decoration-lines
status: complete
depends-on: [native-engine-104]
---

# Native bounded text-decoration lines

## Objective

Extend the existing inherited fixed-cell `text-decoration` owner with
`overline` and `line-through` while preserving the current `none` and
`underline` behavior. Every supported line must remain an immutable text
artifact consumed by the existing clipping, scrolling, opacity, capture, and
software-raster paths.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-052.md`
- `docs/plan/tasks/native-engine-104.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts one inherited bounded `text-decoration` keyword:
`none|underline|overline|line-through`. Matching is case-insensitive. The
default remains `none`; unsupported declarations and unsupported combinations
remain on the existing typed diagnostic path without raw stylesheet echo.
Colors, thickness, style, offsets, blink, CSS-wide keywords, multiple
decoration lines in one declaration, and `text-decoration-line` longhand
semantics remain unsupported.

For supported fixed-cell text runs:

- `underline` retains the current one-pixel line at the fixed glyph-cell
  baseline offset (`origin_y + 7`);
- `overline` paints one pixel at `origin_y - 1`, above the fixed glyph cell;
- `line-through` paints one pixel at `origin_y + 3`, through the fixed glyph
  cell;
- `none` paints no decoration line. The three line positions are deterministic
  integer-pixel choices and may be clipped at the viewport or ancestor clip;
- cascade, inline precedence, DOM inheritance, child `none` clearing, and
  unsupported-value fallback reuse the existing text-decoration owner;
- decoration changes do not alter text width, line formation, wrapping,
  alignment, overflow geometry, hit testing, semantic/source order, or
  accessibility projections;
- the display-list text command carries mutually exclusive line-style state;
  the same state reaches clipping, root scrolling, opacity-layer replay,
  capture, and raster output without a second geometry or paint owner.

The slice remains restricted to the current integer-pixel, fixed-cell,
horizontal-tb implementation. Font metrics, descender-aware positioning,
decoration propagation parity, decoration color/thickness/style/offset,
multiple simultaneous lines, Unicode shaping, bidi, vertical writing, and
browser-wide text conformance remain outside the contract.

## Tradeoffs

- Reusing the existing text-run artifact keeps decoration lines synchronized
  with source text and all current consumers, but fixed offsets can overlap
  glyph pixels or be clipped at the top of a viewport.
- Supporting the two most common additional single-line styles improves local
  fixture coverage without pretending to implement font metrics or the full
  CSS Text Decoration model.
- Keeping one keyword per declaration preserves the current bounded parser and
  avoids a new list-valued cascade or artifact schema for combinations.
- No new crate, dependency, renderer, mutable geometry owner, or default
  feature is introduced; the experiment remains inside `glass-browser`.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, README, and plan docs

## Implementation and verification

Design was recorded in `9002ae13`; implementation is committed as
`ebfefefa`. The implementation keeps the two-crate boundary and adds no
dependency or feature-default change. Final focused gates passed as follows:

- CSS parser/cascade: 2 passed; native decoration integration: 2 passed;
  raster scroll/clip/alpha: 1 passed; unsupported-value diagnostic regression:
  1 passed.
- Full `native-engine` integration suite: 142 passed, 0 failed, 0 ignored.
- Feature-enabled `glass-browser` library suite: 896 passed, 1 ignored.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- Strict all-feature Clippy: passed in 13m37s; no-default-feature Clippy:
  passed in 6m06s; warning-denied workspace rustdoc: passed in 3m11s.
- Locked `glass-dev --bins`: passed in 12m44s. Locked browser and dev
  packages passed; the existing yanked `chacha20 0.10.1` lockfile warning was
  reported by Cargo. The packaged-dependency validator confirmed that the dev
  package resolves `glass-browser` at exactly 0.3.14.
- Locked offline fuzz all-target check: passed in 8m50s.
- Static validators passed: version sync; feature parity (14 capabilities,
  4 targets); TUI shortcut inventory (15 implementation keys, 63 markers);
  documentation depth (93 current guides, 19 substantive contracts);
  read-only adapters (5); reliability matrix (6 scenarios, 4 targets);
  release documentation (519 Markdown files, 83 current documents, 57
  previous-version hits, 591 semantic hits, 0 current-claim failures); and
  Web IR (8 fixtures, 8 scenarios, 11 categories).
- Fresh source-built documentation coverage passed with 519 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, and 22 public
  modules. The installed `/home/ubuntu/.cargo/bin/glass` binaries were not
  used because they are stale 0.3.14 builds with 344 tools; coverage used the
  current source-built binaries instead.

The initial integration compile caught and corrected a test-only five-channel
RGBA expectation before the final focused and full reruns. No production
failure remained. Remote CI remains pending because `main` is local-only; no
release, registry publication, or browser-parity claim is made by this task.

## Verification

The final implementation gate included:

- parser, declaration, cascade, inheritance, inline precedence, and typed
  unsupported-value diagnostics for all four bounded keywords;
- display-list assertions proving exactly one decoration state reaches each
  text run and child `none` clears inherited decoration;
- raster golden assertions for overline, line-through, and underline positions,
  alpha blending, ancestor clipping, viewport clipping, and scroll translation;
- regressions proving decoration does not change text layout, wrapping,
  alignment, overflow, hit testing, or semantic/source order;
- `cargo fmt --all -- --check`, focused and full native integration tests,
  feature-enabled library tests with the documented explicit stack, strict
  all-feature and no-default-feature Clippy, warning-denied rustdoc, locked
  binaries, paired package/dependency gates, fuzz checking,
  documentation/release validators, and exact isolated-target cleanup. The
  isolated `/tmp/glass-105-target` and
  `/tmp/glass-105-coverage-target` trees were measured at 5.9G and 2.1G
  respectively before final reclamation; both are regenerable and are removed
  in the closeout cleanup recorded with this task.

Remote CI remains pending because `main` is local-only. No browser-parity,
release, registry-publication, or remote-certification claim is part of this
task.

## Cleanup

After all gates completed on 2026-09-04 UTC, the exact non-symlink paths
`/tmp/glass-105-target` (5.9G) and
`/tmp/glass-105-coverage-target` (2.1G) were checked for active processes and
open files, then removed with bounded same-filesystem deletion. The exact
regenerable reports `/tmp/glass-105-release-documentation.json` (164K) and
`benchmarks/results/glass-105-web-ir.json` (4.0K) were removed as well. None
remain; the repository and ForgeBuild target directories are 4.0K, the fuzz
target is absent, and the final filesystem check reports 65G available (67%
used).
