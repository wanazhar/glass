---
id: native-engine-076
scope: glass-browser/native-engine/flex-align-content-normal
status: complete
depends-on: [native-engine-075]
---

# Native engine 076: bounded flex align-content normal

## Objective

Extend the completed wrapped-row `align-content` owner with the explicit
`normal` keyword, using the 075 line-box stretch implementation while
preserving the existing two-crate boundary, default-off native feature,
deterministic geometry, and shared layout/paint/hit-test/scroll/capture
consumers.

## Context

Read `docs/INDEX.md`, `docs/architecture/native-engine.md`,
`docs/plan/README.md`, and `docs/plan/analysis/native-engine.md` before
implementation. This slice composes only with the completed
`native-engine-064` through `native-engine-075` flex-row, gap, justification,
visual-order, item-alignment, direction, wrapping, cross-line distribution,
wrap-reverse, and explicit stretch slices.

## Contract

The native CSS grammar adds the non-inherited keyword `normal` to
`align-content`. For an eligible block-level `display:flex` container with a
fixed-width row direction and `flex-wrap:wrap` or `wrap-reverse`, an explicit
`align-content:normal` selects the same bounded line-box expansion as
`align-content:stretch`:

- item sizing, stable `(order, source_index)` sorting, line membership, gap,
  main-axis justification, physical line stacking, and per-line `align-items`
  remain the completed 070-through-075 behavior;
- only positive remainder from an explicit resolved content-box height after
  the originally formed line heights is distributable. The quotient is added
  to every formed line and the first formed lines receive the integer
  remainder pixels;
- expanded line records are rebuilt before the existing second pass. Normal
  and `wrap-reverse` use the same signed complete-artifact translation already
  proven for `stretch`;
- direct item boxes, nested descendants, text runs, display-list entries,
  viewport projection, point hit testing, root overflow, scrolling, and
  capture consume the same final coordinates;
- `nowrap`, auto-height, and undersized explicit-height containers preserve
  their existing geometry. Semantic DOM traversal, source text, node
  references, accessibility order, and keyboard order remain document order;
- `normal` remains non-inherited. A child does not receive its parent's value,
  and an explicit child declaration still wins through the existing selector
  and inline cascade;
- the bounded engine's omitted-value fallback remains `flex-start`. This
  slice adds explicit `normal` without changing every existing wrapped row's
  unspecified geometry; adopting the CSS initial-value behavior globally is a
  later compatibility decision, not an implicit side effect here.

The property remains a bounded explicit alias to the existing line-box
expansion owner, not a general Flexbox or browser-conformance claim.

## Explicit exclusions

This slice does not change the omitted-value default, add `place-content`,
cross-axis gaps, `row-gap`, `column-gap`, flex growth/shrink/basis, auto
margins, column directions, logical direction/RTL mapping, intrinsic or
percentage sizing, nested scrolling, positioned or stacking layout, keyboard
or accessibility reordering, JavaScript, or browser Flexbox parity. It does
not add a second layout owner, change non-flex normal flow, change the default
production Chromium/CDP path, or alter the stable two-crate package topology.

## Tradeoffs and risks

Keeping omitted `align-content` mapped to the established `flex-start` value
preserves all previously certified unspecified-row geometry, but it means the
bounded engine does not yet model the CSS initial-value behavior of `normal`.
Making the explicit keyword a separate computed value keeps parser,
non-inheritance, cascade, and diagnostics behavior auditable; resolving it in
the same layout branch as `stretch` avoids duplicated geometry logic and
prevents normal/reverse artifact consumers from drifting. The slice keeps
integer line expansion and the deliberate first-line remainder bias.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`: add the bounded
  `Normal` computed value, parser, cascade, inline declaration, and diagnostics
  coverage;
- `crates/glass-browser/src/browser/native_engine/layout.rs`: route explicit
  `normal` through the existing stretch line expansion and artifact pass;
- `crates/glass-browser/tests/native_engine.rs`: cover parser/cascade
  acceptance, non-inheritance, explicit normal geometry for normal and
  wrap-reverse rows, equivalence to stretch, auto/smaller heights, `nowrap`,
  descendants, paint, hit testing, scrolling, and semantic/source order;
- architecture/analysis/plan records and issue #40 after implementation
  evidence is complete.

## Verification

- [x] CSS unit tests cover accepted explicit `normal`, omitted-value fallback,
  rejected values, non-inheritance, selector cascade, and inline precedence;
- [x] integration tests cover normal/stretch equivalence for normal and
  wrap-reverse rows, deterministic line expansion, auto/smaller heights,
  `nowrap`, descendants, root overflow, scrolling, hit testing, paint, and
  semantic/source-order preservation;
- [x] the full native integration suite, strict default/native Clippy,
  formatting, whitespace, and documentation validators pass;
- [x] implementation, documentation, and issue #40 checkpoints are committed
  locally; remote CI is not claimed until this branch is pushed;
- [x] exact regenerable Cargo outputs are reclaimed after all validation
  without terminating long-lived Glass processes.

## Completion evidence

Completed locally on 2026-09-02. The design checkpoint is `b6c647e`, the
implementation checkpoint is `e3933b3`, and the documentation closeout is the
follow-up checkpoint recorded with this task update. The implementation stays
inside `glass-browser`; `native-engine` remains default-off and the workspace
still has exactly two installable crates.

Focused and full native validation passed:

```text
cargo test -p glass-browser --features native-engine --test native_engine \
  native_flex_align_content --locked -- --nocapture
4 passed; 0 failed; 0 ignored

cargo test -p glass-browser --features native-engine --lib align_content \
  --locked -- --nocapture
2 passed; 0 failed; 0 ignored

cargo test -p glass-browser --features native-engine --test native_engine \
  --locked -- --nocapture
97 passed; 0 failed; 0 ignored

RUST_MIN_STACK=8388608 cargo test -p glass-browser \
  --features native-engine --lib --locked -- --nocapture
869 passed; 0 failed; 1 ignored

cargo clippy --all-targets --all-features --locked -- -D warnings
cargo clippy --all-targets --no-default-features --locked -- -D warnings
env RUSTDOCFLAGS='-D warnings' cargo doc --all-features --locked --no-deps
cargo build -p glass-dev --bin glass --locked
```

The focused cold integration compile completed in `13m13s`; its tests ran in
`0.45s`. The focused CSS parser/cascade run completed in `6m53.22s` with peak
RSS `2,479,344 KiB`; full native integration completed in `2.97s` with peak
RSS `82,104 KiB`; and the full native library completed in `9.31s` with peak
RSS `81,968 KiB`. Strict all-target Clippy passed in `13m21.78s` with peak RSS
`1,876,640 KiB`; no-default-feature Clippy passed in `6m37.40s` with peak RSS
`1,792,064 KiB`; rustdoc passed in `3m16.73s` with peak RSS `1,632,060 KiB`;
and the `glass-dev` binary build passed in `11m11.55s` with peak RSS
`1,996,176 KiB`. Formatting (`cargo fmt --all -- --check`) and
`git diff --check` passed.

The repository validators passed after the 076 documentation edits: version
sync, feature parity (14 capabilities across 4 targets), release
documentation (490 Markdown documents; 83 current documents; 57 previous-
version hits; 0 current-claim failures), TUI shortcuts, documentation depth
(93 routed/audited guides and 19 substantive contracts), documentation
coverage (490 Markdown files, 345 full-product MCP tools, 100 browser-only,
17 examples, and 22 public modules), reliability matrix, public read-only
adapters, and the 8-fixture/8-scenario/11-category Web IR corpus.

The default-stack large-Clap parser overflow remains a pre-existing harness
follow-up; the complete native library result above uses the documented 8 MiB
test-thread stack and has no test failure. It is not a native-engine-076
failure.

After all validation, no cargo/rustc/rustdoc/clippy process had the target
open. The exact `/home/ubuntu/work/glass/target` generated tree measured
`5.2G` before cleanup and `4.0K` afterward; filesystem free space returned to
`66G`. Shared Cargo registries/toolchains and source files were retained, and
no long-lived Glass process was terminated. Remote CI remains pending because
the checkout is local-only; no remote-green claim is made.
