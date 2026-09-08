---
id: native-engine-192
scope: glass-browser/native-engine/cascade-overflow-important-priority
status: complete
depends-on: [native-engine-191]
---

# Native overflow `!important` cascade priority

## Objective

Extend the bounded author-origin `!important` cascade contract to the
normal-only overflow declarations already consumed by the native clip and
projection owners:

- `overflow`;
- `overflow-x`; and
- `overflow-y`.

## Contract

- Each listed declaration accepts its existing bounded grammar (`hidden`,
  `clip`, or the existing non-clipping `visible|auto|scroll` sentinel) or a
  standalone case-insensitive `revert-layer`, followed by a terminal
  case-insensitive `!important` marker.
- Important candidates outrank normal candidates. Within the bounded
  author-important partition, earliest named layers win, later named layers
  follow, unlayered important candidates are lowest, and inline important
  candidates retain inline precedence in the unlayered important bucket.
- `overflow` expands one parsed value to both independent x/y streams at the
  existing declaration boundary. `overflow-x` and `overflow-y` then retain
  their independent shorthand/longhand source-order behavior and per-axis
  `revert-layer !important` rollback.
- Invalid later values preserve earlier valid values and their importance.
  Existing visible/no-clip fallback, axis-specific clip bits, normal-flow
  layout, root overflow/scroll projection, display-list, raster, capture,
  point-hit, and semantic/source-order consumers remain unchanged.
- No public computed-value or artifact schema changes, dependency changes,
  feature-default changes, crate changes, or third-party browser behavior are
  introduced.

## Boundary and tradeoffs

- Only the three local overflow declarations receive this priority behavior.
  Nested scrolling, scrollbars, `visible`/`auto`/`scroll` used-value parity,
  logical writing modes, multiple origins, transitions, animations, and
  browser-wide overflow conformance remain outside this slice.
- Two private doubled x/y candidate arrays plus three bounded importance bits
  add fixed per-style storage. Shorthand expansion keeps one declaration's
  priority for both axes, while later axis longhands retain their existing
  source-order replacement behavior.
- Reusing `LocalCascadeDeclaration<OverflowValue>`, the existing local layer
  resolver, and current diagnostic path keeps the change bounded. Unsupported
  values remain ignored and diagnosed; no general CSS property registry is
  introduced.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus unit coverage on terminal marker parsing, shorthand expansion,
  independent axis importance, invalid-later preservation, important-over-
  normal priority, reversed named layers, unlayered/inline precedence, and
  `revert-layer !important` rollback.
- Run one native integration fixture through hidden/clip axis projection,
  visible fallback, root overflow/scroll behavior, display-list/raster/capture,
  point-hit, and semantic/source-order outputs.
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

- Design checkpoint: `7e693b79`; implementation and tests: `fc2c461e`
  (`feat(native-engine): honor overflow importance`). The private doubled x/y
  candidate streams and three private shorthand/x/y importance bits extend the
  bounded author-origin partition to `overflow`, `overflow-x`, and `overflow-y`
  without changing public computed values, artifact schemas, dependencies,
  default features, or the two-crate boundary.
- Scoped locked check passed:
  `RUST_MIN_STACK=8388608 CARGO_TARGET_DIR=/tmp/glass-192-focused cargo check
  -q -p glass-browser --features native-engine --tests --locked`.
- Focused parser/cascade coverage passed: 2/2. The dedicated native
  integration regression passed: 1/1. Full native integration passed:
  `230/230`.
- Strict native-feature all-target Clippy passed with `-D warnings`, and
  warning-denied native-feature library rustdoc passed. `cargo fmt --all` and
  `git diff --check` passed.
- The integration fixture covers axis-specific clip projection, shorthand and
  layer rollback, inline important precedence, root overflow/scroll extent,
  display-list clips, raster pixels, PNG dimensions, point-hit routing, and
  semantic/source-order preservation. Unit coverage also records terminal
  marker parsing and invalid-later preservation.
- Synchronized architecture, plan, analysis, product capability, SDK,
  host-RFC, and issue records describe the bounded overflow `!important`
  contract and retain the nested-scroll, used-value, origin, and browser-wide
  overflow boundaries.
- Issue-level final gates, remote CI, push, release, tag, registry
  publication, browser parity, security-boundary certification, and promotion
  remain deferred until the epic's final validation boundary.
