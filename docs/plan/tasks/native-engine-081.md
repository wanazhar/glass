---
id: native-engine-081
scope: glass-browser/native-engine/flex-basis
status: complete
depends-on: [native-engine-080]
---

# Native bounded flex-basis sizing

## Objective

Extend the existing bounded row-flex geometry owner with a deterministic
`flex-basis` input. Explicit integer-pixel bases must feed the shipped
grow/shrink, wrapping, justification, paint, overflow, and hit-test paths
without adding a second sizing owner, fractional metrics, or a workspace
crate/dependency.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts `flex-basis: auto` and a non-negative integer
pixel length from `0px` through `MAX_NATIVE_VIEWPORT_DIMENSION`. The property
is non-inherited and defaults to `auto`; it uses the existing selector
specificity, source order, and inline precedence. Invalid, negative,
unitless, fractional, percentage, `calc()`, `content`, CSS-wide, and
out-of-range values remain unsupported and do not replace an earlier valid
declaration. The `flex` shorthand is not part of this boundary.

For an eligible block-level `display:flex` row or `row-reverse` container:

- `auto` delegates to the existing width/intrinsic item path, preserving the
  current compatibility behavior for items that do not opt into an explicit
  basis;
- an explicit pixel basis overrides the item's `width` as its initial main
  size, converts through the existing content-box or border-box inset helper,
  and applies the existing effective `min-width`/`max-width` constraints;
- explicit bases are not clamped to the container before line formation, so a
  basis wider than the available line can participate in the existing
  shrink-or-overflow decision; margins and resolved gaps remain outside the
  basis and are never resized;
- wrapping and line formation use the constrained explicit or auto base width
  plus margins and column gap; a later grow or shrink pass changes item width
  only after the line is formed;
- the original base width drives the existing bounded `flex-grow` and
  base-width-weighted `flex-shrink` allocations, including max/min freezing;
  `flex-basis:0px` therefore provides a zero base for positive growth while
  still respecting a declared minimum;
- the final width is passed through the existing descendants, content
  rectangles, display-list, raster, overflow, viewport projection, hit
  testing, scrolling, capture, and semantic/source-order consumers.

The property affects only eligible flex-item sizing. It does not change
non-flex width resolution, normal-flow fallback, semantic/source order,
cross-axis sizing, or the explicit-only native-engine selection boundary.

## Tradeoffs

- Integer pixels and a small `auto`/length grammar keep the sizing owner
  deterministic and cheap to build, but percentages, fractional lengths, and
  the `flex` shorthand remain visibly unsupported.
- Explicit bases bypass the old unwrapped available-width clamp so the
  already-shipped shrink path can resolve an oversized basis. This can expose
  overflow when shrink is zero, and it intentionally differs from legacy
  width-only fixtures only when `flex-basis` is present.
- `auto` delegates to the existing width/intrinsic path, including its current
  compatibility clamp, rather than reimplementing the full browser flex base
  size algorithm. That preserves old layouts but is not a browser-conformance
  claim.
- Existing box-sizing and min/max helpers are reused, so the basis remains an
  outer coordinate in the current integer model. The slice does not add
  intrinsic-content measurement, percentage resolution, aspect ratio, or
  automatic minimum-size behavior.
- Basis-driven wrapping remains a formed-line decision; subsequent growth or
  shrink never moves an item between already-formed lines. This keeps line
  records and artifact translation stable at the cost of full Flexbox parity.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

Local evidence captured on 2026-09-02 UTC:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- focused CSS coverage passed 2/2 tests in 13m21s, including bounded
  `auto`/pixel parsing, invalid fallback, specificity, declaration order,
  inline precedence, non-inheritance, and the default value;
- focused flex-basis integration coverage passed 2/2 tests in 5m52s,
  including width override plus growth and descendant geometry, basis-driven
  wrapping, box/min constraints, and explicit overflow;
- the full native integration suite passed 107/107 tests in 3s, and the full
  native library suite passed 879 tests with 1 existing ignored test under
  `RUST_MIN_STACK=8388608`;
- strict `cargo clippy --all-targets --all-features --locked -- -D warnings`
  passed in 13m37s, and the no-default-feature variant passed in 7m16s;
- `RUSTDOCFLAGS='-D warnings' cargo doc --all-features --locked --no-deps`
  passed in 3m04s, and `cargo build -p glass-dev --bin glass --locked`
  passed in 11m21s;
- repository validators passed: version sync at 0.3.14; feature parity at 14
  capabilities across 4 targets; release documentation at 495 Markdown files
  with 0 current-claim failures; TUI at 15 implementation keys and 63
  documentation markers; depth at 93 guides and 19 contracts; coverage at
  495 Markdown files, 345 full-product MCP tools (100 browser-only), 17
  examples, and 22 public modules; reliability at 6 scenarios across 4
  targets; 5 read-only adapters; and Web IR at 8 fixtures, 8 scenarios, and
  11 categories;
- after validation, the exact regenerable Glass target paths were checked for
  active users and open files, then removed: `target` fell from 5.2G to 4.0K;
  `/dev/sda1` moved from 133G used/60G available/70% to 128G used/65G
  available/67%; no build process or target file handle was active;
- implementation checkpoint `299c93f9` and this documentation checkpoint are
  committed locally before the next slice; remote CI remains unclaimed because
  the branch is local-only.
