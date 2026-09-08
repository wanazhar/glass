---
id: native-engine-194
scope: glass-browser/native-engine/box-model-css-wide-resets
status: complete
depends-on: [native-engine-193]
---

# Native box-model CSS-wide reset keywords

## Objective

Add a bounded CSS-wide reset family to the existing physical and
horizontal-tb logical box-model owners without adding a general CSS-wide
cascade engine.

## Contract

- `box-sizing`, physical padding/margin shorthands and longhands, and the
  twelve supported logical padding/margin names accept standalone,
  case-insensitive `initial`, `unset`, and one-author-origin `revert`.
- These reset forms resolve to the existing local initial fallbacks:
  `content-box` for `box-sizing` and zero for every padding or margin edge.
  Margin `auto` remains supported as a distinct existing value.
- Reset forms may carry the existing terminal case-insensitive `!important`
  marker and participate in the existing important-over-normal, reversed
  named-layer, inline-important, invalid-later, and `revert-layer` behavior.
- Reset keywords cannot be mixed with lengths, `auto`, or other shorthand
  tokens. Existing `revert-layer` remains a cascade rollback keyword and is
  not folded into the reset values.
- Logical declarations continue to project block/inline edges through the
  resolved horizontal-tb `ltr`/`rtl` direction before reaching the physical
  edge owners. Existing source-order, geometry, normal-flow/flex, overflow,
  display-list, raster, PNG capture, point-hit, and semantic/source-order
  consumers remain unchanged.

## Boundary and tradeoffs

- This slice deliberately does not implement `inherit`, percentages, negative
  lengths, margin collapsing, positioning, vertical writing modes, additional
  logical properties, multiple origins, transitions, animations, or
  browser-wide CSS-wide conformance.
- `revert` uses the current one-author-origin local fallback rather than
  modeling user-agent, user, animation, or transition origins.
- Reset forms are normalized into the existing finite value streams, so no
  public computed-style enum, transport schema, dependency, feature default,
  renderer, crate, or security-boundary change is needed.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive reset forms, shorthand and
  longhand expansion, mixed-token rejection, terminal `!important`,
  `revert-layer` interaction, invalid-later preservation, physical/logical
  source order, ltr/rtl projection, and independent padding/margin/box-sizing
  fallback values.
- Run one integration fixture through geometry, normal-flow/flex margins,
  overflow/scroll projection, display-list/raster/PNG capture, point-hit, and
  semantic/source-order consumers.
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

- Design checkpoint: `e643a64c`.
- Implementation checkpoint: `5477fb79`.
- Locked native-feature check passed in an isolated task target.
- The focused reset parser/cascade unit and box-model artifact integration
  regression passed.
- The complete native integration target passed: `232 passed; 0 failed; 0
  ignored`.
- Strict native-feature Clippy and warnings-denied rustdoc passed, as did
  formatting and `git diff --check`.
- The integration fixture covered physical and logical reset fallbacks,
  ltr/rtl projection, important precedence, content-box/border-box geometry,
  normal-flow/flex margin consumers, display-list/raster/PNG capture, point-hit,
  and semantic/source-order outputs.
- Static gates passed: release-documentation truth reported 608 Markdown
  documents, 83 current documents, 57 previous-version hits, 706 semantic
  audit hits, and 0 current-claim failures; documentation depth reported 93
  current guides and 19 substantive contracts; the TUI inventory reported 15
  implementation help keys and 63 documentation markers.
- After confirming no Cargo/Rust process and no open handle referenced them,
  the exact `/tmp/glass-194-focused` target (3,400,133,033 bytes; 6,584
  files; 842 directories) and exact
  `/tmp/glass-release-documentation-194.json` report (194,106 bytes) were
  removed with bounded `find -P -xdev -depth -delete`; both paths were
  verified absent. Observed filesystem free space rose from 74G to 77G in
  `df -h` output (rounded).

Remote CI, push, release, tag, registry publication, browser parity,
security-boundary certification, and promotion remain outside this local task.
