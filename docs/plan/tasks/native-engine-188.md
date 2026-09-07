---
id: native-engine-188
scope: glass-browser/native-engine/cascade-local-presentation-important-priority
status: planned
depends-on: [native-engine-187]
---

# Native local presentation `!important` cascade priority

## Objective

Extend the bounded author-origin `!important` cascade contract to the local
presentation declarations that still use the normal-only candidate partition:

- `display`;
- `visibility`; and
- `opacity`.

These properties already have bounded value grammars, local
`revert-layer` support, and shared layout/semantic/paint consumers. This slice
adds priority semantics without changing those owners.

## Contract

- Each listed declaration accepts the existing bounded grammar followed by a
  terminal case-insensitive `!important` marker.
- Important candidates outrank normal candidates. Within the bounded
  author-important partition, earliest named layers win, later named layers
  follow, and unlayered important candidates are lowest. Inline important
  candidates retain inline precedence in the unlayered important bucket.
- Existing `revert-layer` candidates roll back only their current partition
  before lower candidates are considered. Invalid later declarations do not
  erase an earlier valid declaration or its importance.
- `display` continues to resolve to the existing `DisplayValue::Auto` fallback;
  `visibility` continues to resolve its existing hidden/other state; and
  `opacity` continues to resolve an optional 8-bit alpha without inheriting to
  descendants.
- Existing hidden-subtree layout, semantic visibility, opacity-group display
  list, software raster, capture, point-hit, diagnostics, and public transport
  schemas remain the consumers.

## Boundary and tradeoffs

- Only the three listed local presentation properties receive this priority
  behavior. Flex/gap, dimensions/box model, overflow, and other properties
  remain normal-only until separately contracted.
- A private doubled local candidate array and one shared local important-layer
  mapper add bounded per-style storage. This preserves the public computed
  values and avoids leaking cascade provenance into layout, semantic, display,
  or raster artifacts.
- The slice remains one author origin with the existing finite display and
  visibility grammars, bounded opacity quantization, local fixture/data-URL,
  integer-pixel, horizontal-tb, non-table, and non-browser-parity boundaries.
  No dependency, default feature, or crate boundary changes.

## Verification

- Run one locked native-feature `cargo check` in an isolated task target before
  tests.
- Focus unit coverage on marker parsing, important-over-normal priority,
  reversed named layers, unlayered/inline important values, invalid-later
  preservation, independent display/visibility/opacity behavior, and
  `revert-layer !important`.
- Run one native integration fixture through hidden-subtree layout and
  semantics, opacity-group display-list/raster/capture behavior, and
  point-hit consumers.
- Near issue completion retain the full native integration, feature library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, and bounded cleanup gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Cleanup

Only exact task-specific regenerable targets and reports may be removed after
Cargo/Rust process and open-handle checks. Source, durable data, repository
targets, issue snapshots, and unrelated workloads remain untouched.
