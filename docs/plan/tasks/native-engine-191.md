---
id: native-engine-191
scope: glass-browser/native-engine/cascade-box-model-important-priority
status: complete
depends-on: [native-engine-190]
---

# Native box-model `!important` cascade priority

## Objective

Extend the bounded author-origin `!important` cascade contract to the
normal-only local box-model declarations already consumed by the native
geometry owner:

- `box-sizing`;
- physical `padding`, `padding-top`, `padding-right`, `padding-bottom`, and
  `padding-left`; and
- physical `margin`, `margin-top`, `margin-right`, `margin-bottom`, and
  `margin-left`.

## Contract

- Each listed declaration accepts its existing bounded grammar or standalone
  case-insensitive `revert-layer`, followed by a terminal case-insensitive
  `!important` marker. Padding remains finite non-negative integer pixels;
  margin remains finite non-negative integer pixels or `auto`; and
  `box-sizing` remains `content-box|border-box`.
- Important candidates outrank normal candidates. Within the bounded
  author-important partition, earliest named layers win, later named layers
  follow, unlayered important candidates are lowest, and inline important
  candidates retain inline precedence in the unlayered important bucket.
- Padding, margin, and `box-sizing` retain independent candidate streams.
  Shorthand expansion and physical longhand source order remain unchanged;
  shorthand/longhand importance is carried per physical edge. `revert-layer
  !important` rolls back only the current important layer before lower
  candidates are considered.
- Margin `auto` remains private provenance on each physical edge after
  resolution. Existing content-box/border-box conversion, normal-flow and
  flex auto-margin placement, overflow projection, display-list, raster,
  capture, point-hit, and semantic/source-order consumers remain unchanged.
- Valid values and their importance survive invalid later declarations. Public
  computed values, layout boxes, artifact schemas, diagnostics, dependencies,
  default features, and the two-crate boundary remain unchanged.

## Boundary and tradeoffs

- Only the ten listed physical box-model declarations receive this priority
  behavior. Overflow priority, logical edges, percentages, negative edges,
  margin collapsing, positioning, replaced-element sizing, multiple origins,
  transitions, animations, and browser box-model parity remain outside this
  slice.
- Eight private doubled edge candidate arrays plus one doubled `box-sizing`
  array and bounded per-edge importance bits add fixed per-style storage. The
  implementation deliberately keeps `auto` provenance in the existing
  `NativeAutoEdges` path rather than exposing cascade metadata.
- Reusing the existing local declaration grammar and resolver keeps the change
  bounded and preserves current shorthand expansion. Unsupported values remain
  ignored and diagnosed through the existing parser path; no general CSS
  property registry is introduced.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus unit coverage on terminal marker parsing for all box-model families,
  shorthand/longhand edge importance, `auto` provenance, invalid-later
  preservation, important-over-normal priority, reversed named layers,
  unlayered/inline important precedence, independent streams, and
  `revert-layer !important` rollback.
- Run one native integration fixture through content-box/border-box geometry,
  padding origins, normal-flow and flex auto margins, overflow/scroll
  projection, display-list/raster/capture, point-hit, and semantic/source-order
  outputs.
- Near issue completion retain the full native integration, feature library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, workspace all-target/all-feature, and bounded cleanup gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Cleanup

Only exact task-specific regenerable targets and reports may be removed after
Cargo/Rust process and open-handle checks. Source, durable data, repository
targets, issue snapshots, and unrelated workloads remain untouched.

## Completion evidence

- Design checkpoint: `6b2b06c6`; implementation and tests:
  `43e5f8c2` (`feat(native-engine): honor box model importance`). The private
  doubled local candidate arrays and per-edge importance bits cover
  `box-sizing`, physical padding, and physical margin without changing public
  computed values, artifact schemas, dependencies, default features, or the
  two-crate boundary.
- Scoped locked check passed:
  `RUST_MIN_STACK=8388608 CARGO_TARGET_DIR=/tmp/glass-191-focused cargo check
  -q -p glass-browser --features native-engine --tests --locked`.
- Focused parser/cascade coverage passed: 2/2. The dedicated native
  integration regression passed: 1/1. Full native integration passed:
  `229/229`.
- Strict native-feature all-target Clippy passed with `-D warnings`, and
  warning-denied native-feature library rustdoc passed. `cargo fmt --all` and
  `git diff --check` passed.
- The integration fixture covers content-box/border-box geometry, per-edge
  padding and margin origins, normal-flow and flex `auto` margins, overflow
  clipping, display-list fills, raster pixels, PNG dimensions, point-hit
  routing, and semantic/source-order preservation. Invalid later declarations
  remain diagnosed and do not erase earlier valid box-model candidates.
- Synchronized architecture, plan, analysis, product capability, SDK,
  host-RFC, and issue records describe the physical box-model `!important`
  contract and retain the explicit logical-edge/overflow/general-CSS boundary.
- Issue-level final gates, remote CI, push, release, tag, registry
  publication, browser parity, security-boundary certification, and promotion
  remain deferred until the epic's final validation boundary.
