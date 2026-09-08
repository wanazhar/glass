---
id: native-engine-195
scope: glass-browser/native-engine/box-model-inheritance
status: complete
depends-on: [native-engine-194]
---

# Native box-model explicit inheritance

## Objective

Add bounded explicit `inherit` handling to the existing box-model cascade
owners without introducing a general inheritance or CSS-wide keyword engine.

## Contract

- `box-sizing`, physical padding/margin shorthands and longhands, and the
  twelve supported horizontal-tb logical padding/margin names accept standalone,
  case-insensitive `inherit`.
- Physical `inherit` copies the parent computed physical value. Padding and
  `box-sizing` copy their effective values; margin copies both its length and
  private `auto` provenance. A root with no parent receives the existing
  content-box/zero-edge/zero-margin fallbacks.
- Logical `inherit` first reads the parent logical side using the parent’s
  resolved horizontal-tb `ltr`/`rtl` direction, then projects that inherited
  value into the child’s resolved direction. This keeps logical inheritance
  stable when parent and child directions differ.
- Inherited values participate in the existing important-over-normal, reversed
  named-layer, inline-important, invalid-later, source-order, and
  `revert-layer` behavior. A later `revert-layer` can roll back to a lower
  inherited candidate, and an omitted property still uses the local initial
  fallback rather than inheriting implicitly.
- `inherit` cannot be mixed with lengths, `auto`, reset keywords, or other
  shorthand tokens. Existing standalone `initial`, `unset`, `revert`, and
  `revert-layer` semantics remain distinct.

## Boundary and tradeoffs

- This slice does not add implicit inheritance to non-inherited box-model
  properties, percentages, negative lengths, margin collapsing, positioning,
  vertical writing modes, additional logical properties, multiple origins,
  transitions, animations, or browser-wide CSS conformance.
- Parent effective box values and direction are carried only through the
  existing private style walk. Public computed-style accessors, transport
  schemas, renderer ownership, dependencies, feature defaults, crate
  boundaries, and security boundaries remain unchanged.
- Margin `auto` is retained as private provenance during inheritance so flex
  consumers do not mistake an inherited `auto` for a zero length. No new
  layout owner is introduced.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive physical and logical
  `inherit`, parent/child direction changes, shorthand and longhand expansion,
  mixed-token rejection, root fallback, terminal `!important`,
  `revert-layer` rollback, invalid-later preservation, and inherited margin
  `auto` provenance.
- Run one integration fixture through parent/child geometry, content-box and
  border-box sizing, normal-flow/flex margin consumers, logical ltr/rtl
  projection, overflow/scroll projection, display-list/raster/PNG capture,
  point-hit, and semantic/source-order consumers.
- Near issue completion retain the full native integration, feature library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, workspace all-target/all-feature, and bounded cleanup gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Cleanup

Only exact task-specific regenerable targets and reports may be removed after
Cargo/Rust process and open-handle checks. Source, durable data, repository
targets, issue snapshots, and unrelated workloads remain untouched.

## Completion evidence

- Design checkpoint: `a61b9c5f`.
- Implementation checkpoint: `0caad64b`.
- The locked native-feature test-target check passed in the isolated
  `/tmp/glass-195-focused` target before test execution.
- The focused inheritance parser/cascade unit and public integration regression
  passed.
- The complete native integration target passed: `233 passed; 0 failed; 0
  ignored`.
- Strict native-feature Clippy and warnings-denied rustdoc passed, as did
  formatting and `git diff --check`.
- The integration fixture covered physical and logical parent/child inheritance,
  ltr/rtl direction projection, inherited margin `auto` provenance,
  content-box/border-box sizing, display-list/raster/PNG capture, point-hit,
  and semantic/source-order outputs. The reset fixture also records the
  correct 15px logical reset geometry and expresses important-vs-normal
  precedence across separate cascade rules.
- Static gates passed: release-documentation truth reported 609 Markdown
  documents, 83 current documents, 57 previous-version hits, 707 semantic
  audit hits, and 0 current-claim failures; documentation coverage reported
  609 Markdown files, 345 full-product MCP tools (100 browser-only), 17
  examples, and 22 public modules; documentation depth reported 93 current
  guides and 19 substantive contracts; feature parity, version sync, and TUI
  shortcut inventory also passed with 14 capabilities across 4 targets,
  synchronized 0.3.14 package versions, and 15 implementation help keys with
  63 documentation markers.
- After confirming no Cargo/Rust process and no open handle referenced them,
  the exact `/tmp/glass-195-focused` target (5,581,929,120 bytes; 8,892
  files; 1,071 directories) and exact
  `/tmp/glass-release-documentation-195.json` report (194,347 bytes) were
  removed with bounded `find -P -xdev -depth -delete`; both paths were
  verified absent. Observed filesystem free space rose from 72G to 77G in
  `df -h` output (rounded).

Remote CI, push, release, tag, registry publication, browser parity,
security-boundary certification, and promotion remain outside this local task.
