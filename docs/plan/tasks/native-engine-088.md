---
id: native-engine-088
scope: glass-browser/native-engine/align-items-normal
status: complete
depends-on: [native-engine-087]
---

# Native bounded align-items normal

## Objective

Extend the bounded parent `align-items` grammar with the explicit `normal`
keyword. In the supported flex formatting context, `normal` must use the
completed stretch path for auto-sized direct items while retaining the
specified keyword in computed style and diagnostics. The native backend's
omitted-value fallback remains its existing bounded `flex-start` behavior.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- [CSS Box Alignment Level 3: flex-container normal behavior](https://drafts.csswg.org/css-align/#distribution-values)
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

The native CSS grammar accepts the explicit, non-inherited parent value
`align-items:normal` alongside the completed `flex-start`, `center`,
`flex-end`, and `stretch` values. Parsing remains case-insensitive and accepts
one token only. Computed style retains `Normal` as a distinct value so
specified intent remains observable; layout resolves it only at the supported
flex-item used-value boundary. The omitted-value fallback remains bounded
`flex-start`; this slice does not silently change native-engine's initial
behavior.

For an eligible direct element child of the existing fixed-width row or
row-reverse flex layout whose computed `align-self` is `auto`, parent
`align-items:normal` behaves as the completed `align-items:stretch` path:

- an omitted `height` fills the existing line cross size minus vertical
  margins, never shrinking below the natural height from the shared child
  layout pass;
- physical padding and border insets, bounded pixel `min-height`, and bounded
  pixel `max-height` constrain the resulting outer box through the existing
  box-model owner;
- single-row explicit parent content height and wrapped formed line height
  after line-gap and `align-content` distribution remain authoritative;
- an explicit bounded `height` is preserved and uses the bounded flex-start
  placement fallback.

Explicit child `align-self` values remain authoritative: `flex-start`,
`center`, `flex-end`, and `stretch` retain their existing behavior. Child
`align-self:auto` remains the only auto-resolution path covered here;
`align-self:normal` is reserved for a later item-value slice. `normal` is
non-inherited and must not appear as a child `align-items` value merely because
the parent uses it. Cascade specificity, source order, inline precedence,
and invalid-declaration retention remain unchanged.

Outside the bounded row/row-reverse flex layout, this slice does not claim
the general block, grid, absolute-positioned, writing-mode, or logical-axis
semantics of `normal`. CSS-wide keywords, baseline, logical start/end,
safe/unsafe, multi-token, fractional, percentage, auto-margin,
column-direction, intrinsic-sizing, and other unsupported forms remain typed
diagnostics or out of scope. No change to `place-content` or `align-content`
is included.

The resulting layout box and complete descendant artifact range must remain
consistent across line overflow, display-list paint, software rasterization,
viewport projection, hit testing, scrolling, capture, and semantic/source
order. The slice reuses the existing stretch helper and introduces no second
cross-axis geometry representation.

## Tradeoffs

- A distinct computed `Normal` value preserves specified-value provenance and
  makes future layout-mode-specific handling possible, at the cost of one
  explicit used-value mapping in the flex owner.
- Mapping `normal` to stretch only for supported flex items captures the
  standards-defined flex behavior without pretending that the bounded native
  backend implements every layout mode. The tradeoff is intentionally
  explicit diagnostics outside this boundary.
- Reusing the 086/087 used-size helper keeps padding, borders, min/max bounds,
  descendants, paint, hit testing, and capture on one geometry owner.
- The omitted fallback remains `flex-start` to protect existing native
  fixtures and avoid an implicit initial-value behavior change. The native
  value is therefore a deliberate bounded extension, not a complete CSS
  conformance claim.
- No new dependency, crate, runtime, or artifact pipeline is added; the
  native backend remains default-off inside `glass-browser`, and Chromium/CDP
  remains the production runtime.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

- Design checkpoint: `162543f1`.
- Implementation checkpoint: `19d8d3f6`. The parser/cascade coverage passed
  2/2 tests; the clean isolated build and test measurement was 813.31 seconds
  with 2,498,732 KiB peak RSS. The focused shared-layout integration case
  passed 1/1 in 363.48 seconds with 2,117,948 KiB peak RSS. It covers the
  normal-to-stretch used-size equivalence, explicit child overrides,
  descendants, paint, and hit testing.
- The full native integration suite passed 115/115 in 3.70 seconds. The full
  native library suite passed 887 tests with 1 ignored in 7.66 seconds.
- Strict all-feature Clippy passed with warnings denied in 832.70 seconds with
  1,886,332 KiB peak RSS. The strict no-default-feature matrix passed in
  406.58 seconds with 1,794,844 KiB peak RSS. Workspace rustdoc with
  `RUSTDOCFLAGS="-D warnings"` passed in 194.88 seconds with 1,631,880 KiB
  peak RSS. The locked `glass-dev` binary build passed in 675.73 seconds with
  1,997,464 KiB peak RSS.
- Formatting, `git diff --check`, version synchronization at `0.3.14`,
  feature parity (14 capabilities across 4 targets), release documentation
  (502 Markdown files and zero current-claim failures), TUI shortcut inventory
  (15 implementation keys and 63 documentation markers), documentation depth
  (93 current guides and 19 substantive contracts), reliability (6 scenarios
  across 4 targets), public read-only adapters (5), and Web IR (8 fixtures,
  8 scenarios, and 11 categories with runtime goldens) all passed. Live
  documentation coverage passed in 4.34 seconds with 38,180 KiB peak RSS:
  502 Markdown files, 345 full-product MCP tools (100 browser-only),
  17 examples, and 22 public modules.
- The exact temporary `/tmp/glass-088-target` tree reached 5.3G during the
  isolated binary inventory build. It was inspected after all processes
  exited, had no open files, and was removed in full. The project
  `/home/ubuntu/work/glass/target` remains only its root directory at 4.0K;
  `/dev/sda1` is 129G used, 64G available, and 67% full. Shared Cargo
  registries and toolchains were retained. Long-lived Glass processes were
  not terminated.
- Issue [#40](https://github.com/wanazhar/glass/issues/40) was updated with
  the design, implementation, evidence, and cleanup state. Remote CI is not
  claimed because this branch remains local-only; no push, release, tag,
  registry publication, or browser-parity certification is claimed.
