---
id: native-engine-188
scope: glass-browser/native-engine/cascade-local-presentation-important-priority
status: complete
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

## Evidence

- Implementation checkpoint: `ca0b47bd`.
- `RUST_MIN_STACK=8388608 CARGO_TARGET_DIR=/tmp/glass-188-focused cargo check -q -p glass-browser --features native-engine --tests --locked` passed.
- Focused parser/cascade unit tests passed: 2/2.
- Focused native integration regression passed: 1/1.
- Full native integration passed: 226/226.
- Strict native-feature Clippy with `-D warnings` passed.
- Warning-denied native-feature rustdoc passed.
- `cargo fmt --all` and `git diff --check` passed.

The integration fixture verified the resolved display and visibility hidden
owners, semantic hidden projection, point hit testing, important-over-normal
priority, reversed named-layer priority, inline important precedence,
inline `revert-layer`, invalid-later preservation, reduced-opacity display
groups, software source-over replay, and PNG capture. No public schema,
dependency, default-feature, or crate-boundary change was made.

## Cleanup

- Exact target inventory before deletion: `/tmp/glass-188-focused`,
  5,383,944,304 bytes, 9,096 files, and 1,180 directories.
- Exact report inventory before deletion:
  `/tmp/glass-release-documentation-188.json`, 191,907 bytes.
- `ps` found no active `cargo`, `rustc`, `rustdoc`, or Clippy process, and
  `timeout 30s lsof -nP +D /tmp/glass-188-focused` found no open handles.
- Removed only the exact task target and report with bounded
  `find -P <path> -xdev -depth -delete` commands.
- Verified both paths are absent after deletion.
- Final static release-audit rerun report inventory:
  `/tmp/glass-release-documentation-188-final.json`, 191,907 bytes; it was
  removed with the same bounded exact-path command and verified absent.
- `/tmp` available bytes increased from 76,641,693,696 to 82,054,823,936
  (5,413,130,240 bytes, approximately 5.04 GiB reclaimed).
- Source, durable data, repository targets, issue snapshots, and unrelated
  workloads were not touched.
