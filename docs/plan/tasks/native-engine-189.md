---
id: native-engine-189
scope: glass-browser/native-engine/cascade-flex-gap-important-priority
status: planned
depends-on: [native-engine-188]
---

# Native flex and gap `!important` cascade priority

## Objective

Extend the bounded author-origin `!important` cascade contract to the
normal-only flex and gap declarations already consumed by the native layout
owner:

- `justify-content`, `align-items`, `align-self`, `align-content`,
  `place-content`;
- `flex-direction`, `flex-wrap`, `flex-flow`;
- `order`, `flex-grow`, `flex-shrink`, `flex-basis`, and `flex`; and
- `gap`, `row-gap`, and `column-gap`.

## Contract

- Each listed declaration accepts the existing bounded grammar followed by a
  terminal case-insensitive `!important` marker.
- Important candidates outrank normal candidates. Within the bounded
  author-important partition, earliest named layers win, later named layers
  follow, unlayered important candidates are lowest, and inline important
  candidates retain inline precedence in the unlayered important bucket.
- `place-content`, `flex-flow`, and `flex` preserve their existing bounded
  expansion into component owners while carrying the one declaration's
  importance to every valid component.
- Gap shorthand/longhand source-order precedence remains independent per axis;
  `revert-layer !important` blocks only its current important layer before
  lower candidates are considered. Invalid later declarations preserve earlier
  valid component values and importance.
- Existing finite values, fallback defaults, flex geometry, line formation,
  directionality, gap spacing, overflow projection, paint, raster, capture,
  point-hit, semantic, and public schemas remain unchanged.

## Boundary and tradeoffs

- Only the listed flex/gap declarations receive this priority behavior.
  Dimensions/box model and overflow remain normal-only until separately
  contracted; multiple origins, transitions, animations, and browser-wide
  Flexbox/CSS conformance remain outside the native claim.
- Private doubled flex candidate arrays and important gap partitions add
  bounded per-style storage. No importance provenance leaks into computed
  values or layout/artifact schemas.
- The slice retains the existing bounded fixed-pixel, eligible-row/column,
  horizontal-tb, integer geometry, local fixture/data-URL, and two-crate
  boundaries. No dependency, default feature, or crate boundary changes.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus unit coverage on marker parsing, shorthand expansion, invalid-later
  preservation, important-over-normal priority, reversed named layers,
  unlayered/inline important values, independent gap axes, and
  `revert-layer !important`.
- Run one native integration fixture through flex row/column/wrap placement,
  gap spacing, overflow/scroll projection, display-list/raster/capture, and
  point-hit consumers.
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
