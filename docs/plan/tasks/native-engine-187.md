---
id: native-engine-187
scope: glass-browser/native-engine/cascade-text-presentation-important-priority
status: complete
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

## Evidence

Implementation and slice-local certification are complete at `65883117`.

- `cargo fmt --all -- --check` and `git diff --check` passed.
- The locked native-feature check passed with
  `CARGO_TARGET_DIR=/tmp/glass-187-focused cargo check -q -p glass-browser --features native-engine --tests --locked`.
- Focused parser/cascade coverage passed: 2 library unit tests and 1 native
  integration test for text-presentation `!important` priority.
- The full `native_engine` integration target passed 225/225 tests.
- The feature-enabled `glass-browser` library target passed 1,004 tests with 1
  intentionally ignored.
- Strict all-target Clippy with `-D warnings` passed for `glass-browser` with
  `native-engine`.
- Warning-denied rustdoc passed for `glass-browser` with `native-engine`.
- The release documentation truth audit passed over 601 Markdown documents
  with 83 current documents, 57 previous-version hits, 697 semantic-audit
  hits, and 0 current-claim failures.
- Documentation coverage passed with 601 Markdown files, 345 full-product MCP
  tools (100 browser-only), 17 examples, and 22 public modules; documentation
  depth passed with 93 current guides and 19 substantive contracts; feature
  parity passed for 14 capabilities across 4 targets; the TUI shortcut
  inventory passed with 15 implementation help keys and 63 documentation
  markers; and version sync passed at 0.3.14.
- Issue-level workspace, release, security/fuzz, paired-crate, package, and
  remote-CI gates remain deferred to the final issue #40 certification
  boundary.

## Paths

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Cleanup

Only exact task-specific regenerable targets and reports may be removed after
Cargo/Rust process and open-handle checks. Source, durable data, repository
targets, issue snapshots, and unrelated workloads remain untouched.

Task-specific cleanup completed after all Cargo/Rust processes and open handles
exited: `/tmp/glass-187-focused` measured 6,226,073,293 bytes across 9,720
files and 1,183 directories, and `/tmp/glass-release-documentation-187.json`
measured 191,868 bytes. The process and open-handle checks were empty.
Bounded exact-path deletion removed only those regenerable paths; post-delete
absence checks passed. Available filesystem bytes moved from 75,929,100,288 to
82,136,805,376, an observed increase of 6,207,705,088 bytes. Source,
repository targets, durable data, unrelated workloads, and issue snapshots
were preserved.
