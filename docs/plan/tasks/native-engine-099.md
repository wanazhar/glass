---
id: native-engine-099
scope: glass-browser/native-engine/flex-directionality
status: complete
depends-on: [native-engine-098]
---

# Native bounded Flexbox directionality

## Objective

Make inherited `direction:ltr|rtl` participate in the existing bounded
Flexbox axis mapping. Horizontal rows must start from the inline start of the
current direction, while columns keep their vertical main axis and reflect
their horizontal cross axis and wrapped line stacking. Preserve one layout and
artifact owner, source/semantic order, and the 098 line-local auto-margin
behavior.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-098.md`
- [CSS Flexible Box Layout Module Level 1: flex-direction](https://www.w3.org/TR/css-flexbox-1/#flex-direction-property)
- [CSS Flexible Box Layout Module Level 1: axis mappings](https://www.w3.org/TR/css-flexbox-1/#axis-mappings)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts only the bounded inherited values
`direction:ltr` and `direction:rtl`. The computed value is inherited through
the existing DOM style walk, defaults to `ltr`, participates in normal cascade
and inline-style precedence, and remains a typed computed state. Unsupported
CSS-wide, bidi-override, writing-mode, and malformed forms retain the existing
diagnostic/fallback behavior.

Layout consumes the direction state only in the existing eligible Flexbox
owners, under the current horizontal-tb and integer-pixel assumptions:

- for `flex-direction:row`, the physical main start is left for `ltr` and
  right for `rtl`; `row-reverse` swaps that mapping. Existing gaps, flex
  grow/shrink/basis, justify distribution, line-local auto margins, overflow,
  and integer remainder rules run in that logical main direction;
- for `flex-direction:column|column-reverse`, the vertical main axis and its
  reverse mapping are unchanged. `direction:rtl` changes the horizontal
  cross-start to the right, so `align-items`/`align-self` start/end and
  horizontal auto margins use the existing cross-axis owner with the reflected
  physical mapping;
- for wrapped columns, the horizontal line stacking direction is the XOR of
  `wrap-reverse` and inherited `rtl`. Wrapped rows retain their vertical
  cross-axis mapping because direction does not change the horizontal-tb block
  axis;
- direction combines with `row-reverse`, `column-reverse`, and `wrap-reverse`
  without sorting or reversing DOM children. Semantic, keyboard, and source
  order remain the document order, and every line/item subtree continues
  through the same layout, overflow, scroll, hit-test, display-list, raster,
  viewport, capture, and semantic consumers;
- nested flex owners inherit direction independently through the existing
  parent chain. A descendant may override the inherited value with normal
  selector or inline cascade precedence;
- non-flex normal flow, direct-text glyph order, Unicode bidi/shaping,
  `text-align:start|end`, logical properties, vertical writing modes, grid,
  floats, and browser-wide directionality remain outside this slice and retain
  their existing bounded fallback behavior.

## Tradeoffs

- Keeping the feature in the existing Flexbox owner makes RTL rows and column
  cross-axis placement useful without inventing a second coordinate system, but
  it deliberately does not claim general bidi or inline formatting behavior.
- Direction is inherited in computed style even where layout ignores it. This
  keeps the cascade state coherent for nested flex descendants while avoiding a
  false claim that the fixed-cell text rasterizer performs Unicode bidi.
- Direction changes physical start/end mapping, not source order. This follows
  the Flexbox accessibility model and avoids changing semantic references or
  keyboard traversal while visual placement changes.
- `rtl` is implemented only for the current horizontal-tb model. Vertical
  writing modes and logical physical-edge properties remain explicit future
  work rather than being approximated as horizontal RTL.
- The implementation combines existing boolean reverse flags at the owner
  boundary. This keeps wrapping, auto margins, and artifact translation
  line-local and deterministic, at the cost of leaving unsupported layout
  modes on their current fallback path.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

Implementation checkpoint: `3bf658e8` (`feat(native-engine): map inherited
flex directionality`). The focused and full local gates passed against the
isolated `CARGO_TARGET_DIR=/tmp/glass-099-target` build root:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- CSS direction parser/cascade tests: 5 passed, 886 filtered;
- focused direction integration tests: 1 non-flex fallback test and 5
  direction/flex tests passed;
- full native integration suite: 135 passed, 0 failed;
- feature-enabled browser library suite with `RUST_MIN_STACK=8388608`: 890
  passed, 1 existing ignored, 0 failed;
- `cargo clippy --workspace --all-targets --all-features --locked -- -D
  warnings` passed;
- `cargo clippy -p glass-browser --no-default-features --all-targets --locked
  -- -D warnings` passed;
- warning-denied workspace rustdoc passed;
- locked `glass-dev` binary build passed;
- documentation coverage passed at 513 Markdown files, 345 full-product MCP
  tools (100 browser-only), 17 examples, and 22 public modules;
- locked `glass-browser` and `glass-dev` packages passed, and the packaged
  `glass-dev` archive resolves `glass-browser` exactly at 0.3.14;
- locked fuzz fetch and offline all-targets check passed in 9m22s;
- the package gate emitted only the pre-existing warning that `chacha20
  v0.10.1` is yanked in the registry; it did not fail the gate.

The documentation/release validators also passed:

- version and feature parity are synchronized at 0.3.14;
- release documentation truth: 513 Markdown documents, 83 current documents,
  57 previous-version hits, 576 semantic audit hits, and 0 current-claim
  failures;
- TUI shortcut inventory: 15 implementation help keys and 63 documentation
  markers;
- documentation depth: 93 current guides and 19 substantive contracts;
- reliability matrix: 6 scenarios across 4 targets;
- public read-only adapter inventory: 5 adapters;
- Web IR corpus: 8 fixtures, 8 scenarios, and 11 categories with live
  evidence verification.

After all validation completed, exact process and open-file checks found no
consumer of `/tmp/glass-099-target`. The validated non-symlink target was
removed with bounded same-filesystem deletion, reclaiming 6.6G; the 160K
`/tmp/glass-099-release-documentation.json` report was also removed. The
workspace `/home/ubuntu/work/glass/target` and `/home/ubuntu/work/forgebuild/target`
were preserved at 4.0K each, `fuzz/target` remained absent, and disk usage
changed from 58G available / 70% used to 65G available / 67% used.

The implementation covers typed inherited direction, selector and inline
cascade precedence, unsupported-value diagnostics, row/row-reverse physical
main-start mapping, column cross-axis and wrapped-line reflection, physical
auto margins, nested artifacts, and non-flex/source-text fallback behavior.
Direction remains bounded to the current horizontal-tb Flexbox owners; this
task does not claim Unicode bidi/shaping, logical properties, text-align
start/end, vertical writing, grid, or browser-wide directionality.

Static documentation/release validators and exact isolated-target cleanup are
complete. Remote CI remains pending because `main` is local-only. No
browser-parity, release, registry-publication, or remote-certification claim is
part of this task.
