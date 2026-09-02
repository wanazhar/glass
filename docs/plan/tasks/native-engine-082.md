---
id: native-engine-082
scope: glass-browser/native-engine/flex-shorthand
status: complete
depends-on: [native-engine-081]
---

# Native bounded flex shorthand expansion

## Objective

Complete the bounded row-flex sizing input by expanding a small, explicit
`flex` shorthand grammar into the already-shipped `flex-grow`, `flex-shrink`,
and `flex-basis` fields. The slice must preserve one computed-style and layout
owner, declaration-order behavior, deterministic integer sizing, and the
existing visual/interaction consumers.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts these bounded shorthand forms:

- `none`, expanding to `0 0 auto`;
- `auto`, expanding to `1 1 auto`;
- one non-negative integer `N`, expanding to `N 1 0px`;
- two tokens `N M`, expanding to `N M 0px` when both are bounded integer
  factors, or to `N 1 B` when the second token is `auto` or a bounded pixel
  basis;
- three tokens `N M B`, where `N` and `M` are bounded integer factors and `B`
  is `auto` or a bounded non-negative integer pixel length.

Grow and shrink factors are bounded to `0..=1024`; pixel bases use the
existing `0px..=MAX_NATIVE_VIEWPORT_DIMENSION` bound. The property is
non-inherited. `initial`, `inherit`, `unset`, `revert`, percentages,
fractional factors, unitless bases, negative values, `calc()`, `content`, and
other token shapes remain unsupported and produce bounded diagnostics.

The shorthand expands at the declaration's existing specificity, source
order, and inline precedence. Within one declaration block, a valid shorthand
sets all three components, a later valid longhand replaces only its component,
and a later valid shorthand replaces all three. Invalid shorthand or longhand
values do not erase an earlier valid component. Across rules, each expanded
component uses the existing per-property cascade winner; inline declarations
remain strongest.

The expanded values feed the completed flex-basis base-size, flex-grow
free-space, and flex-shrink deficit owners. The final item widths continue
through existing wrapping, justification, descendants, content rectangles,
display-list, raster, overflow, viewport projection, hit testing, scrolling,
capture, and semantic/source-order consumers. The shorthand does not change
semantic order or make native-engine selection implicit.

## Tradeoffs

- A deliberately small grammar makes common fixed-row shorthand useful while
  keeping diagnostics and compile cost bounded; full CSS token ambiguity,
  `flex: 1 1 0%` percentage semantics, and browser conformance remain outside
  the claim.
- The one-number form maps its implicit zero basis to `0px`, retaining the
  integer coordinate model rather than introducing a percentage-valued
  intermediate. This is deterministic but not a claim of exact browser used
  values.
- Parsing expands directly into the existing component fields, so later
  longhands in the same block can override individual components without a
  second cascade representation. The tradeoff is that unsupported importance
  or CSS-wide token semantics remain fail-closed rather than partially modeled.
- `none` and `auto` are explicit presets, while `initial` remains unsupported
  under the native engine's existing CSS-wide-value policy. This keeps the
  reset surface consistent with the rest of the bounded parser.
- Since layout already consumes the three computed components, this slice
  adds no new geometry algorithm. It inherits the prior slices' limitations
  around intrinsic sizing, fractional metrics, columns, auto margins, and
  multiple independent flex contexts.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

Local evidence captured on 2026-09-02 UTC:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- focused CSS coverage passed 2/2 shorthand parser and cascade tests in
  13m48s, including every accepted form, invalid fallback, declaration-order
  longhand overrides, inline precedence, and non-inheritance;
- focused flex-shorthand integration coverage passed 1/1 test in 6m03s,
  including zero-basis growth, descendant geometry, display-list paint, and
  hit testing;
- the full native integration suite passed 108/108 tests in 3.06s, and the
  full native library suite passed 881 tests with 1 existing ignored test in
  4.69s under `RUST_MIN_STACK=8388608`;
- strict `cargo clippy --all-targets --all-features --locked -- -D warnings`
  passed in 13m32s, and the no-default-feature variant passed in 6m39s;
- `RUSTDOCFLAGS='-D warnings' cargo doc --all-features --locked --no-deps`
  passed in 3m23s, and `cargo build -p glass-dev --locked` passed in 11m21s;
- repository validators passed: version sync at 0.3.14; feature parity at 14
  capabilities across 4 targets; release documentation at 496 Markdown files
  with 0 current-claim failures; TUI at 15 implementation keys and 63
  documentation markers; depth at 93 guides and 19 substantive contracts;
  coverage at 496 Markdown files, 345 full-product MCP tools (100
  browser-only), 17 examples, and 22 public modules; reliability at 6
  scenarios across 4 targets; 5 public read-only adapters; and the Web IR
  corpus at 8 fixtures, 8 scenarios, and 11 categories with runtime goldens
  verified;
- after validation, the exact Glass `target` inventory was 5.3G, with no active
  Cargo/Rust/rustdoc/Clippy process and no open target file. Only regenerable
  paths were removed: `target` fell to 4.0K; `/dev/sda1` moved from 134G
  used/60G available/70% to 128G used/65G available/67%. Shared Cargo
  registries/toolchains and long-lived processes were retained;
- design checkpoint `71d1060d` and implementation checkpoint `40f6fb1c` are
  committed locally; this task is the documentation closeout checkpoint.
  Issue #40 remains OPEN and remote CI remains unclaimed because the branch is
  local-only.
