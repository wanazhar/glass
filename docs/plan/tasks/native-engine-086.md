---
id: native-engine-086
scope: glass-browser/native-engine/align-self-stretch
status: complete
depends-on: [native-engine-085]
---

# Native bounded align-self stretch

## Objective

Extend the existing bounded non-inherited `align-self` contract with
`stretch` for eligible direct flex items. An auto-height item must fill the
existing flex-line cross size through the same layout box, descendant,
display-list, raster, overflow, projection, hit-test, scrolling, and capture
owners. The slice must not create a second cross-axis geometry representation.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts `align-self:stretch` in addition to the
completed `auto`, `flex-start`, `center`, and `flex-end` values. The property
remains non-inherited and scoped to eligible direct element children of the
existing fixed-width row or row-reverse flex layout.

For an item whose computed `height` is omitted, `stretch` resolves its used
outer height to the existing line cross size minus its vertical margins. The
result reuses the current bounded box model, including physical padding and
border insets plus bounded pixel `min-height` and `max-height` constraints. It
never shrinks an item below the natural height already produced by the shared
layout pass. In a single non-wrapping row, the existing explicit parent
content height is the line cross size; in wrapped rows, the existing formed
line height remains authoritative.

For an item with an explicit bounded `height`, `stretch` does not rewrite the
declared size. The item uses the bounded flex-start placement fallback, rather
than inheriting the parent `align-items` offset. This keeps the slice
deterministic without adding an auto-size or margin-resolution algorithm.

The expanded value preserves existing specificity, source order, inline
precedence, and invalid-declaration retention. A valid later longhand wins at
its existing cascade position; an invalid later declaration does not erase an
earlier valid `align-self` value. Unsupported CSS-wide, baseline, normal,
logical start/end, safe/unsafe, multi-token, and other unsupported forms
remain typed diagnostics.

The parent `align-items` grammar remains bounded to its existing
`flex-start|center|flex-end` values; this slice does not add
`align-items:stretch`, auto margins, column directions, intrinsic or
fractional sizing, or general Flexbox conformance. The stretched root box and
its complete descendant artifact range must remain consistent across layout,
line overflow, display-list paint, software rasterization, viewport
projection, hit testing, scrolling, capture, and semantic/source order.

## Tradeoffs

- Stretch is implemented as a used-size adjustment after the existing natural
  child layout and line-size calculation. That preserves one line owner and
  keeps descendant flow deterministic, but it intentionally does not model
  every CSS auto-margin, baseline, min-content, or intrinsic-sizing rule.
- Explicit heights stay fixed and top-aligned under the bounded fallback. This
  avoids silently changing an author-declared size, at the cost of not claiming
  full CSS used-value behavior for every non-auto cross size.
- Existing physical padding, borders, and bounded min/max heights constrain
  the stretched outer box. Logical properties, percentages, fractional
  lengths, and writing-mode semantics remain outside the native boundary.
- No new dependency or geometry owner is introduced. The native backend stays
  default-off inside `glass-browser`, and Chromium/CDP remains the production
  runtime.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The implementation checkpoint is `5ea2c8d1`, following design checkpoint
`f92b7b9a`; strict layout lint cleanup is `2c07c749`. Focused CSS
parser/cascade coverage passed 2/2 tests in 6m53s after rebuilding from a
clean target. Focused shared-layout integration coverage passed 1/1 test with
the compile phase finishing in 31.90s and the test running in 0.05s. The
integration case covers auto-height stretch, explicit-height preservation,
box-model and max-height bounds, descendant coordinates, paint, and hit
testing.

The full native integration suite passed 112/112 tests in 2.54s. The full
native library suite passed 887 tests with 1 ignored in 4.72s under
`RUST_MIN_STACK=8388608`; the initial default-stack attempt hit the existing
deep-suite stack limit and was not treated as passing evidence. Strict
all-feature Clippy passed with warnings denied in 8m01s; the no-default-feature
matrix passed with warnings denied in 7m12s. Rustdoc with
`RUSTDOCFLAGS='-D warnings'` passed in 3m17s, and the locked `glass-dev`
binary build passed in 11m28s. Formatting and `git diff --check` passed.

The first full release-certification attempt reached the environment-dependent
Rust Analyzer test and failed to receive diagnostics; the exact test passed
immediately on retry, and the fresh full workspace test stage passed. The
final `scripts/release-certify.sh` run passed all remaining release,
documentation, package, dependency, and fuzz gates in 730.17s, with peak RSS
of 1,564,156 KiB. Its repository evidence was: version `0.3.14`; feature
parity 14 capabilities across 4 targets; release documentation 500 Markdown
files with 0 current-claim failures; TUI 15 implementation help keys and 63
documentation markers; documentation depth 93 current guides and 19
substantive contracts; documentation coverage 500 Markdown files, 345
full-product MCP tools (100 browser-only), 17 examples, and 22 public
modules; reliability 6 scenarios across 4 targets; 5 public read-only
adapters; and Web IR 8 fixtures, 8 scenarios, and 11 categories with runtime
goldens verified. Both packages were packaged and `glass-dev` resolved
`glass-browser` exactly at `0.3.14`.

After certification, the exact regenerable
`/home/ubuntu/work/glass/target` output fell from 8.4G to 4.0K and
`/dev/sda1` moved from 138G used/56G available/72% to 129G used/65G
available/67%. No Cargo/Rust writer, live target path, or Git lock remained
at cleanup time. Three long-lived Glass processes held already-deleted
executable mappings; they were retained and not terminated. Shared Cargo
registries/toolchains were retained. Remote CI remains pending because the
branch is local-only; no push, release, tag, registry publication, or
browser-parity certification is claimed.
