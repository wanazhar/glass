---
id: native-engine-187
scope: glass-browser/native-engine/cascade-text-presentation-important-priority
status: planned
depends-on: [native-engine-186]
---

# Native bounded text-presentation `!important` cascade priority

## Objective

Extend the bounded author-origin `!important` cascade contract to the
supported text-flow and text-decoration declarations that still use the
normal-only candidate partition:

- `white-space`, `text-align`, `text-align-last`, `text-justify`, and
  `direction`;
- the supported text-decoration line, style, skip-ink, skip-spaces,
  thickness, and underline-offset declarations;
- `text-transform`, `font-weight`, `font-style`, `word-break`,
  `text-overflow`, `vertical-align`, `text-indent`, `word-spacing`,
  `letter-spacing`, and `line-height`.

`background-color`, `color`, and `text-decoration-color` already own their
private important partitions and are unchanged by this slice.

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
- Inherited properties resolve their winning value before descendants consume
  the existing `NativeInheritedStyle`; local properties retain their current
  fallback and non-inheritance behavior.
- Existing text layout, decoration paint, source order, hit testing, scroll,
  capture, diagnostics, and public transport schemas remain the consumers.

## Boundary and tradeoffs

- Only the listed supported text properties receive this priority behavior.
  Display/visibility/opacity, flex/gap, dimensions/box model, overflow, and
  other properties remain normal-only until separately contracted.
- A private doubled candidate array and one shared layer mapper are used for
  this family. This adds bounded per-style storage but avoids duplicating
  value types, changing public computed-style fields, or leaking cascade
  provenance into artifacts.
- The slice remains one author origin with the existing local fixture/data-URL,
  fixed-cell, integer-pixel, horizontal-tb, non-table, and non-browser-parity
  boundaries. No dependency, default feature, or crate boundary changes.

## Verification

- Run one locked native-feature `cargo check` in an isolated task target before
  tests.
- Focus unit coverage on marker parsing, important-over-normal priority,
  reversed named layers, unlayered/inline important values, invalid-later
  preservation, independent decoration declarations, inherited/local
  fallbacks, and `revert-layer !important`.
- Run one native integration fixture through text flow, decoration artifacts,
  source order, layout, raster, capture, and inherited-style propagation.
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
