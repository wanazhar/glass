---
id: native-engine-077
scope: glass-browser/native-engine/flex-row-gap
status: complete
depends-on: [native-engine-076]
---

# Native engine 077: bounded flex row-gap

## Objective

Add explicit cross-line `row-gap` spacing to the completed wrapped fixed-width
flex-row owner. The slice must feed the same line records that already own
`align-content`, `wrap-reverse`, `align-items`, overflow, display-list,
hit-testing, scrolling, and capture output. It must remain inside
`glass-browser`, behind the default-off `native-engine` feature, and preserve
the two-installable-crate workspace boundary.

## Context

Read `docs/INDEX.md`, `docs/architecture/native-engine.md`,
`docs/plan/README.md`, and `docs/plan/analysis/native-engine.md` before
implementation. This slice composes with the completed `native-engine-064`
through `native-engine-076` flex-row, main-axis gap, justification,
visual-order, item-alignment, direction, wrapping, cross-line distribution,
wrap-reverse, stretch, and normal slices.

## Contract

The bounded native CSS grammar adds the non-inherited `row-gap` property as a
single non-negative integer-pixel value in the existing
`0..=MAX_NATIVE_VIEWPORT_DIMENSION` range. An explicit valid `row-gap` applies
only to eligible block-level `display:flex` containers using row direction and
`flex-wrap:wrap` or `wrap-reverse`:

- formed lines retain the existing stable `(order, source_index)` membership,
  item widths, main-axis `gap`, `justify-content`, and per-line
  `align-items` behavior;
- a row gap is inserted between adjacent formed line boxes before
  `align-content` computes positive explicit cross-axis free space. The
  occupied cross size is therefore `sum(line heights) + row_gap * (line_count -
  1)`, with saturating integer arithmetic;
- `align-content:flex-start|center|flex-end|space-between|space-around|
  space-evenly|stretch|normal` distributes only the remainder left after
  those explicit row gaps. `stretch` and `normal` expand line boxes without
  consuming or duplicating the explicit gap; `space-between` adds its
  distributed remainder in addition to the explicit gap;
- `wrap-reverse` reflects the complete row records, including the explicit
  row-gap space, through the existing signed artifact translation. Direct
  boxes, descendants, text runs, display-list commands, viewport projection,
  hit testing, root overflow, scrolling, and capture all consume the final
  coordinates;
- `row-gap` is non-inherited. A child does not receive its parent's value, and
  selector and inline cascade precedence remain the existing bounded rules;
- omitted or invalid `row-gap` remains `0`. Existing `gap` behavior remains
  the established main-axis item spacing for this checkpoint, so current
  single-row and horizontal wrap geometry does not silently change;
- non-flex flow, `nowrap`, auto-height rows without a second line, and rows
  whose explicit height is smaller than their occupied content preserve their
  existing geometry except where an actual inter-line row gap is present;
- semantic DOM traversal, source text, node references, accessibility order,
  and keyboard order remain document order.

The property is a bounded explicit cross-line spacing primitive, not a claim
of complete CSS `gap` shorthand, grid, or browser Flexbox conformance.

## Explicit exclusions

This slice does not add `column-gap`, multi-value `gap`, percentage or
fractional gap values, implicit gap expansion from the existing `gap`
property, flex growth/shrink/basis, auto margins, column directions, logical
direction/RTL mapping, intrinsic sizing, percentage sizing, nested scrolling,
positioned or stacking layout, JavaScript, or browser parity. It does not
change the default production Chromium/CDP path, non-flex normal flow, the
stable two-crate package topology, or the existing default `row-gap:0`
fallback.

## Tradeoffs and risks

Adding row-gap to the occupied line size before `align-content` is important:
otherwise centered, evenly distributed, or stretched rows would double-count
the same cross-axis space and produce visibly incorrect line origins. Keeping
the existing one-value `gap` as main-axis-only for this checkpoint avoids
changing every previously certified wrapped-row golden, but it knowingly
leaves the CSS shorthand incomplete. A later shorthand/column-gap slice can
choose a declaration-order model deliberately instead of hiding that
compatibility change inside this geometry patch. Saturating arithmetic keeps
large bounded values safe, while integer spacing preserves deterministic
pixel output at the cost of the same deliberate rounding behavior already
used by `align-content`.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`: add the bounded
  computed `row_gap` value, getter, parser, cascade, inline declaration, and
  diagnostics coverage without inheriting it;
- `crates/glass-browser/src/browser/native_engine/layout.rs`: insert row-gap
  space between provisional wrapped line records and include it once in
  cross-axis occupied-size/free-space calculations;
- `crates/glass-browser/tests/native_engine.rs`: cover parser/cascade
  acceptance, invalid fallback, non-inheritance, normal and wrap-reverse
  geometry, interaction with all completed `align-content` values, explicit
  height overflow, paint, hit testing, and semantic/source-order preservation;
- architecture/analysis/plan records and issue #40 after implementation
  evidence is complete.

## Verification

- [x] CSS unit tests cover accepted bounded pixels, zero, rejected units/
  ranges, non-inheritance, selector cascade, inline precedence, and
  diagnostics;
- [x] integration tests cover two-line row-gap geometry, wrap-reverse,
  `align-content` remainder math, omitted/invalid fallback, auto/smaller
  heights, no-op single-line behavior, descendants, paint, hit testing,
  overflow/scrolling, and semantic/source order;
- [x] the full native integration suite, strict default/native Clippy,
  formatting, whitespace, rustdoc, and repository documentation validators
  pass;
- [x] implementation, documentation, and issue #40 checkpoints are committed
  locally; remote CI is not claimed until this branch is pushed;
- [x] exact regenerable Cargo outputs are reclaimed after all validation
  without terminating long-lived Glass processes.

## Completion evidence

Completed locally on 2026-09-02. The design checkpoint is `99d70ef`, the
implementation checkpoint is `4b27b15`, and the documentation closeout is the
follow-up checkpoint recorded with this task update. The implementation stays
inside `glass-browser`; `native-engine` remains default-off and the workspace
still has exactly two installable crates.

Focused and full native validation passed:

```text
cargo test -p glass-browser --features native-engine --test native_engine \
  native_flex_row_gap --locked -- --nocapture
3 passed; 0 failed; 0 ignored

cargo test -p glass-browser --features native-engine --lib row_gap \
  --locked -- --nocapture
2 passed; 0 failed; 0 ignored

cargo test -p glass-browser --features native-engine --test native_engine \
  --locked -- --nocapture
100 passed; 0 failed; 0 ignored

RUST_MIN_STACK=8388608 cargo test -p glass-browser \
  --features native-engine --lib --locked -- --nocapture
871 passed; 0 failed; 1 ignored

cargo clippy --all-targets --all-features --locked -- -D warnings
cargo clippy --all-targets --no-default-features --locked -- -D warnings
env RUSTDOCFLAGS='-D warnings' cargo doc --all-features --locked --no-deps
cargo build -p glass-dev --bin glass --locked
```

The first focused integration compile completed in `13m11s`; the final
focused rerun completed in `21.79s` with a `0.18s` test phase and peak RSS of
`610,220 KiB`. The focused CSS parser/cascade run completed in `7m37.18s`
with peak RSS `2,449,412 KiB`; full native integration completed in `4.32s`
with peak RSS `81,972 KiB`; and the full native library completed in `7.41s`
with peak RSS `81,844 KiB`. Strict all-target Clippy passed in `13m14.76s`
with peak RSS `1,868,724 KiB`; no-default-feature Clippy passed in `6m56.37s`
with peak RSS `1,796,416 KiB`; rustdoc passed in `3m16.81s` with peak RSS
`1,634,484 KiB`; and the `glass-dev` binary build passed in `11m22.34s`
with peak RSS `1,986,416 KiB`. Formatting (`cargo fmt --all -- --check`) and
`git diff --check` passed.

The repository validators passed after the 077 implementation: version sync,
feature parity (14 capabilities across 4 targets), release documentation (491
Markdown documents; 83 current documents; 57 previous-version hits; 0
current-claim failures), TUI shortcuts, documentation depth (93 routed/audited
guides and 19 substantive contracts), documentation coverage (491 Markdown
files, 345 full-product MCP tools, 100 browser-only, 17 examples, and 22
public modules), reliability matrix, public read-only adapters, and the
8-fixture/8-scenario/11-category Web IR corpus. The malformed `row-gap`
diagnostic fixture now exercises the recognized-property/unsupported-value
path; `column-gap` remains the explicit unsupported-property sentinel.

The default-stack large-Clap parser overflow remains a pre-existing harness
follow-up; the complete native library result above uses the documented 8 MiB
test-thread stack and has no test failure. It is not a native-engine-077
failure.

After all validation, no cargo/rustc/rustdoc/clippy process had the target
open. The exact project-generated `/home/ubuntu/work/glass/target` tree was
reclaimed after validation, while shared Cargo registries/toolchains and
source files were retained and no long-lived Glass process was terminated.
Remote CI remains pending because the checkout is local-only; no remote-green
claim is made.
