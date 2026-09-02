---
id: native-engine-087
scope: glass-browser/native-engine/align-items-stretch
status: complete
depends-on: [native-engine-086]
---

# Native bounded align-items stretch

## Objective

Extend the existing bounded parent `align-items` grammar with an explicit
`stretch` value. An eligible direct flex item whose `align-self` remains
`auto` must reuse the completed 086 used-size path, so auto-height children
fill their formed line cross size while every complete subtree artifact stays
owned by the shared layout result.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the explicit, non-inherited parent value
`align-items:stretch` in addition to the completed `flex-start`, `center`,
and `flex-end` values. The parser remains case-insensitive and accepts one
value only. The omitted-value fallback remains the existing bounded
`flex-start`; this slice does not silently change the native backend's
initial-value behavior.

For an eligible direct element child of the existing fixed-width row or
row-reverse flex layout whose computed `align-self` is `auto`, parent
`align-items:stretch` resolves through the same used-size operation as the
completed explicit `align-self:stretch` contract:

- an omitted `height` fills the existing line cross size minus vertical
  margins, never shrinking below the natural height already produced by the
  shared child layout pass;
- physical padding and border insets, bounded pixel `min-height`, and bounded
  pixel `max-height` constrain the resulting outer box through the existing
  box-model owner;
- in a single non-wrapping row, the existing explicit parent content height
  is the line cross size; in wrapped rows, the existing formed line height
  after line-gap and `align-content` distribution remains authoritative;
- an explicit bounded `height` is preserved and uses the bounded flex-start
  placement fallback, rather than being rewritten or offset by the parent.

An explicit child `align-self` value continues to override the parent:
`flex-start`, `center`, and `flex-end` retain their existing placement, while
`stretch` uses the same 086 used-size path. The parent value is not inherited
as a child `align-items` declaration. Specificity, source order, inline
precedence, non-inheritance, and invalid-declaration retention remain the
existing cascade rules: an invalid later `align-items` declaration does not
erase an earlier valid winner.

Unsupported CSS-wide, baseline, normal, logical start/end, safe/unsafe,
multi-token, fractional, percentage, and other unsupported forms remain
typed diagnostics. This slice does not add `align-items` auto margins,
column-direction flex, intrinsic sizing, writing-mode semantics, or general
Flexbox conformance.

The stretched root box and its complete descendant artifact range must remain
consistent across layout, line overflow, display-list paint, software
rasterization, viewport projection, hit testing, scrolling, capture, and
semantic/source order. No second cross-axis geometry representation is
allowed.

## Tradeoffs

- Resolving `align-self:auto` to the parent's `stretch` value and reusing the
  086 helper keeps one used-size owner and makes explicit and inherited-by-
  resolution stretch behavior observable through the same downstream
  artifacts. It intentionally does not model every CSS auto-margin,
  baseline, intrinsic, or percentage rule.
- Explicit heights remain fixed and top-aligned under the bounded fallback.
  This protects author-declared dimensions while declining full CSS used-value
  parity for all cross-size combinations.
- The omitted `align-items` fallback stays `flex-start` to avoid changing the
  behavior of existing native fixtures and to keep this extension explicit.
  The tradeoff is that native-engine's initial value is not a claim about the
  standards default.
- Physical integer-pixel padding, borders, and min/max dimensions remain the
  only supported constraints. Logical properties, fractional metrics, real
  font metrics, and writing modes remain outside the boundary.
- No new dependency, crate, geometry owner, or artifact pipeline is added;
  the native backend remains default-off inside `glass-browser`, and
  Chromium/CDP remains the production runtime.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

- Design checkpoint: `4708f663`.
- Implementation checkpoint: `e0d051ce`.
- `cargo test --locked -p glass-browser --features native-engine --lib
  align_items -- --nocapture`: 2 passed; the clean-build measurement was
  810.31 seconds with 2,478,956 KiB peak RSS.
- Focused native integration: 2 passed in the final cached run (23.87
  seconds); the full `native_engine` integration suite passed 114/114 in
  3.72 seconds.
- The full native library suite passed 888 tests with 1 ignored in 7.56
  seconds under `RUST_MIN_STACK=8388608`.
- Strict all-feature Clippy passed in 819.30 seconds with 1,880,040 KiB peak
  RSS; strict no-default-features Clippy passed in 408.80 seconds with
  1,790,364 KiB peak RSS.
- Workspace rustdoc with `-D warnings` passed in 201.73 seconds with
  1,631,976 KiB peak RSS. The locked `glass-dev` binary build passed in
  672.05 seconds with 1,999,548 KiB peak RSS.
- Release certification reached the complete browser suite and the
  `glass-dev` suite. The first run reported 364 passed and one failure in the
  environment-sensitive Rust Analyzer diagnostics probe; the exact isolated
  retry passed 1/1 in 5.90 seconds. The other reached gates reported feature
  parity 14/4, release documentation 501 Markdown files with zero current
  claim failures, TUI 15/63, and documentation depth 93/19. Because the
  first run exited at that test failure, later certification commands were
  rerun as the focused/static gates above rather than represented as a clean
  single-script exit.
- The exact regenerable `/home/ubuntu/work/glass/target` tree was inventoried
  at 8.4G after validation and removed only after all build processes exited;
  it now contains only its root directory (4.0K). No remote CI result is
  claimed because this branch remains local-only.

Issue #40 was updated with the design, implementation, evidence, and cleanup
state. The source of truth remains the issue and this task record; remote CI
is still pending until the branch is intentionally pushed.
