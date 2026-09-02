---
id: native-engine-083
scope: glass-browser/native-engine/flex-flow-shorthand
status: complete
depends-on: [native-engine-082]
---

# Native bounded flex-flow shorthand expansion

## Objective

Complete the bounded row-flex direction and line-formation inputs by expanding
the small `flex-flow` shorthand into the already-shipped `flex-direction` and
`flex-wrap` fields. The slice must preserve one computed-style and layout
owner, CSS-like declaration-order behavior, deterministic line formation, and
the existing visual/interaction consumers.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts these bounded shorthand forms, with ASCII
whitespace separating tokens and keywords matched case-insensitively:

- one direction token `row` or `row-reverse`, expanding the omitted wrap
  component to its initial `nowrap` value;
- one wrap token `nowrap`, `wrap`, or `wrap-reverse`, expanding the omitted
  direction component to its initial `row` value;
- two tokens containing exactly one direction and one wrap token in either
  order.

The property is non-inherited. `initial`, `inherit`, `unset`, `revert`,
`column`, duplicate direction or wrap tokens, unsupported token shapes, and
other CSS-wide or logical-direction forms remain unsupported and produce
bounded diagnostics.

The shorthand expands at the declaration's existing specificity, source
order, and inline precedence. Within one declaration block, a valid shorthand
sets both components, including the initial value for an omitted component; a
later valid longhand replaces only its component; a later valid shorthand
resets both components; and an invalid shorthand or longhand does not erase an
earlier valid component. Across rules, each expanded component uses the
existing per-property cascade winner; inline declarations remain strongest.

The expanded values feed the completed physical row/reverse placement and
nowrap/wrap/wrap-reverse line-formation owners. Final boxes continue through
existing gaps, justification, flex sizing, descendants, content rectangles,
display-list, raster, overflow, viewport projection, hit testing, scrolling,
capture, and semantic/source-order consumers. The shorthand does not change
semantic order or make native-engine selection implicit.

## Tradeoffs

- The deliberately small grammar covers common row-flow authoring while
  keeping token classification and diagnostics bounded; `column`,
  `column-reverse`, logical direction, and full CSS token-list behavior remain
  outside the claim.
- A one-token form resets the omitted component to its initial bounded value
  (`row` or `nowrap`), matching shorthand reset behavior rather than leaving a
  stale longhand from the same declaration block.
- Parsing expands directly into the existing component fields, so valid
  longhands can override one component without a second cascade representation.
  The tradeoff is fail-closed rejection of unsupported CSS-wide and duplicate
  token semantics.
- Since direction and wrapping already own geometry, this slice adds no new
  line, coordinate, artifact, or dependency algorithm. It inherits the prior
  limitations around columns, logical direction/RTL, intrinsic sizing,
  fractional metrics, and browser Flexbox parity.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

The design checkpoint is `80c6836e`. The implementation checkpoint is
`0291bf90`, followed by the strict-Clippy fix checkpoint `16e6d9aa`.

- focused CSS parser/cascade coverage passed 2/2; focused flex-flow integration
  coverage passed 1/1 after correcting the fixture to the shared margin/gap
  geometry (`x=7`, `y=11` for the wrapped item);
- the full native integration suite passed 109/109, and the full native
  library suite passed 883 with 1 existing ignored test under
  `RUST_MIN_STACK=8388608`;
- strict all-feature Clippy passed with warnings denied after the
  `question-mark` lint fix; no-default-feature Clippy also passed with
  warnings denied; rustdoc passed with `-D warnings`; and the locked
  `glass-dev` build passed;
- repository validators passed: version sync at `0.3.14`; feature parity at
  14 capabilities across 4 targets; release documentation at 497 Markdown
  documents with 0 current-claim failures; TUI at 15 implementation help keys
  and 63 documentation markers; depth at 93 guides and 19 contracts; coverage
  at 497 Markdown files, 345 full-product MCP tools (100 browser-only), 17
  examples, and 22 public modules; reliability at 6 scenarios across 4
  targets; 5 public read-only adapters; and Web IR at 8 fixtures, 8 scenarios,
  and 11 categories;
- after validation, exact Glass regenerable target output was checked for
  active Cargo/Rust writers and open files, then removed; shared Cargo
  registries/toolchains and long-lived Glass processes were retained. Final
  target size and filesystem headroom are recorded in the issue #40 closeout
  comment;
- issue #40 remains open for the next dependency-ordered slice. Remote CI is
  not claimed because this branch is local-only; no push, release, tag,
  registry publication, or browser-parity certification is implied.
