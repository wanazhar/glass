---
id: native-engine-080
scope: glass-browser/native-engine/flex-shrink
status: complete
depends-on: [native-engine-079]
---

# Native bounded flex-shrink allocation

## Objective

Extend the existing bounded row-flex geometry owner with deterministic
negative-free-space allocation. The slice makes common fixed-width rows honor
the CSS `flex-shrink` contract without adding a second layout engine,
fractional metrics, or a new workspace crate/dependency.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts `flex-shrink` as a non-negative decimal integer
from `0` through `1024`. The property is non-inherited, defaults to `1`, and
uses the existing selector specificity, source order, and inline precedence.
Invalid, negative, fractional, unit-bearing, CSS-wide, and out-of-range values
remain unsupported and do not replace an earlier valid declaration. A factor
of `0` is the explicit opt-out from shrinking.

For an eligible block-level `display:flex` row or `row-reverse` container, the
existing computed outer width after padding, borders, min/max constraints, and
margins remains each item's flex base width. Wrapping and line formation still
use those base widths and the resolved column gap. After a line is formed, if
its base items, margins, and gaps exceed the available width, only item widths
are reduced; margins and gaps are not reduced or redistributed.

Each positive shrink factor receives a weight equal to its integer factor times
its original flex base width. The deficit is allocated with bounded `u64`
intermediates and deterministic prefix-floor integer shares in visual/source
order. An item cannot shrink below its effective outer `min-width`; reaching
that floor freezes the item and redistributes its remaining deficit among the
remaining positive weighted items. If all eligible items reach their minimum,
or all factors are zero, the line remains intentionally overflowing and the
existing reverse/scroll/viewport behavior handles it.

Shrink occurs before `justify-content`, after base-size line formation, and is
mutually exclusive with the 079 positive-growth pass. If the line has no
negative free space, the prior growth/justification behavior remains. The final
width is passed through the existing descendants, content rectangles,
display-list, raster, overflow, viewport projection, hit testing, scrolling,
capture, and semantic/source-order consumers.

This slice explicitly excludes `flex-basis`, the `flex` shorthand, fractional
shrink factors, reflow based on shrunk widths, auto margins, column directions,
percentage/intrinsic sizing changes, multiple independent flex formatting
contexts, and browser Flexbox conformance. It does not alter semantic order or
make native-engine selection implicit.

## Tradeoffs

- Defaulting to the CSS initial factor `1` makes negative free space useful, but
  existing overflow fixtures must opt out with `flex-shrink:0` when they are
  specifically testing legacy overflow reachability.
- Integer factors and prefix-floor shares preserve the current bounded,
  deterministic coordinate model, but decimal CSS shrink values remain
  visibly unsupported.
- Weighting by the original base width matches the important proportional
  behavior while keeping the algorithm small; it does not implement the full
  CSS flexing freeze algorithm or intrinsic sizing.
- Shrinking after line formation keeps the current line owner stable and makes
  wrapped rows predictable, but a grown or shrunk item never changes which
  already-formed line contains it.
- Minimum freezing protects the existing min/max box contract, but a line can
  still overflow when its floors plus margins and gaps cannot fit.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

Local evidence captured on 2026-09-02 UTC:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- focused CSS coverage passed 2/2 tests, including bounded parsing, invalid
  fallback, specificity, declaration order, inline precedence,
  non-inheritance, and the default factor of 1;
- focused flex-shrink integration coverage passed 2/2 tests in 6m06s,
  including base-width weighting, minimum freezing and redistribution, wrapped
  line stability, descendant geometry, and explicit zero-shrink overflow;
- the full native integration suite passed 105/105 tests in 26s; six existing
  fixed-overflow regression fixtures were made explicit with `flex-shrink:0`
  so their stated reachability contracts remain stable under the new CSS
  default;
- the full native library suite passed 877 tests with 1 existing ignored test
  under `RUST_MIN_STACK=8388608`;
- strict `cargo clippy --all-targets --all-features --locked -- -D warnings`
  passed in 13m09s, and the no-default-feature variant passed in 6m57s;
- `RUSTDOCFLAGS='-D warnings' cargo doc --all-features --locked --no-deps`
  passed in 3m23s, and `cargo build -p glass-dev --bin glass --locked` passed
  in 12m06s;
- repository validators passed: version sync at 0.3.14; feature parity at 14
  capabilities across 4 targets; release documentation at 494 Markdown files
  with 0 current-claim failures; TUI at 15 implementation keys and 63
  documentation markers; depth at 93 guides and 19 contracts; coverage at 494
  Markdown files, 345 full-product MCP tools, 17 examples, and 22 public
  modules; reliability at 6 scenarios across 4 targets; 5 read-only adapters;
  and Web IR at 8 fixtures, 8 scenarios, and 11 categories;
- implementation checkpoint `b1414931` and this documentation checkpoint are
  committed locally before the next slice; remote CI remains unclaimed because
  the branch is local-only.
- after validation, the exact regenerable Glass target paths were checked for
  active users and open files, then removed: `target` fell from 5.2G to 4.0K;
  `/dev/sda1` moved from 133G used/60G available/70% to 128G used/65G
  available/67%; no build process or target file handle was active.
