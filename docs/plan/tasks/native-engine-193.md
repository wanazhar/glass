---
id: native-engine-193
scope: glass-browser/native-engine/logical-box-model-edges
status: complete
depends-on: [native-engine-192]
---

# Native logical box-model edges

## Objective

Add a bounded horizontal-tb logical projection for the physical box-model
edges already owned by the native layout engine:

- `padding-block` and `padding-inline` two-side shorthands;
- `padding-block-start`, `padding-block-end`, `padding-inline-start`, and
  `padding-inline-end`; and
- the corresponding `margin-block`, `margin-inline`, and four logical margin
  longhands.

## Contract

- Padding accepts one or two finite non-negative integer pixel values and the
  standalone case-insensitive `revert-layer` keyword. Margin accepts one or
  two finite non-negative integer pixel values, `auto`, and standalone
  case-insensitive `revert-layer`.
- A two-side shorthand maps its first value to the logical start side and its
  second value to the logical end side. One value fills both logical sides.
- Under the existing horizontal-tb assumption, block-start/end project to
  physical top/bottom. Inline-start/end project to physical left/right for
  `direction:ltr` and right/left for `direction:rtl`. The existing inherited
  direction resolver remains the only writing-mode input.
- Logical and physical declarations share the existing per-edge candidate
  streams. Important candidates retain important-over-normal ordering,
  reversed named-layer priority, inline-important precedence, invalid-later
  preservation, and per-edge `revert-layer` rollback. Declaration order inside
  one stylesheet rule is retained when logical and physical edge declarations
  compete.
- Margin `auto` remains private provenance on each projected physical edge.
  Existing content-box/border-box conversion, normal-flow and flex placement,
  overflow projection, display-list, raster, PNG capture, point-hit, and
  semantic/source-order consumers remain unchanged.

## Boundary and tradeoffs

- This slice covers only the twelve logical padding/margin names above. It
  does not add logical `box-sizing`, percentages, negative lengths, margin
  collapsing, positioning, vertical writing modes, other writing-mode axes,
  CSS-wide `inherit`/`unset`/`initial`/`revert`, or browser-wide box-model
  conformance.
- Logical values are stored in four private bounded candidate streams for
  padding and margin, then projected after the inherited direction is resolved.
  This adds fixed per-style cascade state but does not expose logical
  provenance in public computed values or artifacts.
- The physical padding/margin declaration metadata gains bounded declaration
  order so that physical/logical declarations in one rule follow source order;
  this repairs only the existing local edge tie and does not broaden the
  physical value grammar.
- No dependency, feature default, renderer, transport schema, crate, or
  security boundary changes.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus unit coverage on one/two-value expansion, logical side mapping,
  `auto` margin and `revert-layer` parsing, terminal `!important`, invalid
  later preservation, physical/logical declaration order, reversed named
  layers, inline-important precedence, and independent padding/margin edges.
- Run one native integration fixture through ltr/rtl projection, physical and
  logical competition, content-box/border-box geometry, normal-flow and flex
  auto margins, overflow/scroll projection, display-list/raster/PNG capture,
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

- Design checkpoint: `03bcf403`.
- Implementation checkpoint: `3e78c246`.
- Locked native-feature check passed in an isolated task target.
- The focused parser/cascade unit and logical-edge integration regression passed.
- The complete native integration target passed: `231 passed; 0 failed; 0
  ignored`.
- Strict native-feature Clippy and warnings-denied rustdoc passed, as did
  formatting and `git diff --check`.
- The integration fixture covered ltr/rtl projection, physical/logical
  competition, content-box geometry, normal-flow and flex auto margins,
  overflow/scroll projection, display-list/raster/PNG capture, point-hit, and
  semantic/source-order outputs.
- Static gates passed: release-documentation truth reported 607 Markdown
  documents, 83 current documents, 57 previous-version hits, 704 semantic
  audit hits, and 0 current-claim failures; documentation depth reported 93
  current guides and 19 substantive contracts; the TUI shortcut inventory
  reported 15 implementation help keys and 63 documentation markers.
- After confirming no Cargo/Rust process and no open handle referenced them,
  the exact `/tmp/glass-193-focused` target (3,864,645,685 bytes; 6,973 files;
  844 directories) and exact `/tmp/glass-release-documentation-193.json`
  report (193,648 bytes) were removed with bounded `find -P -xdev -depth
  -delete`. Both paths were verified absent; filesystem free space increased
  from the observed 73G rounded value to 77G.

Remote CI, push, release, tag, registry publication, browser parity,
security-boundary certification, and promotion remain outside this local task.
