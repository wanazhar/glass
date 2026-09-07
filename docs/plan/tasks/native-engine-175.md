---
id: native-engine-175
scope: glass-browser/native-engine/cascade-physical-border-radius-corner-longhands
status: complete
depends-on: [native-engine-174]
---

# Native bounded physical `border-radius` corner longhands

## Objective

Adds the four physical corner longhands to the native CSS surface:
`border-top-left-radius`, `border-top-right-radius`,
`border-bottom-right-radius`, and `border-bottom-left-radius`. Each property
accept one bounded non-negative integer-pixel radius or one standalone
CSS-wide keyword, then feed the existing four-corner radius owner used by
layout, display-list replay, rasterization, PNG capture, point hit testing,
and semantics.

This is the smallest follow-up to the completed physical `border-radius`
shorthand and the horizontal-tb logical border family. It closes the
physical corner-declaration gap without adding logical corner mapping,
elliptical geometry, percentages, or a second rounded-rendering path.

## Context

Native-engine-020 established one-to-four-value physical `border-radius`
expansion and rounded consumers. Native-engine-145 added bounded
`revert-layer`; native-engine-172 added `inherit`, `unset`, `initial`, and
one-author-origin `revert`; and native-engine-174 completed the bounded
horizontal-tb logical border family. The current radius state is already a
public `NativeBorderRadius` with top-left, top-right, bottom-right, and
bottom-left values, but only the complete shorthand can populate it.

Normative references:

- <https://www.w3.org/TR/css-backgrounds-3/#border-radius>
- <https://www.w3.org/TR/css-cascade-5/#defaulting-keywords>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-174.md`
- `docs/plan/tasks/native-engine-172.md`
- `docs/plan/tasks/native-engine-020.md`

## Contract

### Supported declarations

- The four physical corner longhands accept one bounded non-negative integer
  `Npx` value using the existing dimension bound.
- Each corner longhand accepts standalone, case-insensitive `inherit`,
  `unset`, `initial`, `revert`, and `revert-layer` using the existing private
  radius CSS-wide declaration model. CSS-wide keywords cannot be mixed with
  dimensions.
- The existing `border-radius` shorthand remains one-to-four bounded
  integer-pixel values plus its already-supported standalone CSS-wide forms.
  Shorthand and corner longhands can compose in one declaration block using
  normal per-corner source order.
- Values outside the bounded grammar remain unsupported and preserve the last
  valid declaration in the existing diagnostic/fallback contract. This slice
  does not add percentages, negative or fractional values, slash-separated
  elliptical radii, variables, or arbitrary CSS tokenization.

### Cascade and resolution

- Radius declarations are collected as four private candidate streams, one
  for each physical corner in the existing top-left, top-right, bottom-right,
  bottom-left order.
- A shorthand projects the same parsed four-corner value into all four
  streams. A corner longhand projects only its selected stream. The existing
  rule order, selector specificity, inline precedence, and bounded named-layer
  resolver decide the winning candidate.
- Declaration order is retained independently for every corner so
  `border-radius: 1px 2px 3px 4px; border-top-left-radius: 9px` and the
  reverse order follow the same shorthand/longhand behavior already used by
  physical border sides. A later shorthand resets all four corners; a later
  corner longhand overrides only its corner.
- `revert-layer` rolls back only the selected corner for a corner longhand.
  A shorthand `revert-layer` rolls back all four projected corner streams.
  `inherit` copies only the selected effective parent corner; reset forms and
  ordinary omission retain the existing zero-corner fallback.
- No declaration-only enum or sentinel reaches `NativeComputedStyle`, the
  public radius value, layout boxes, display commands, capture bytes, raster
  pixels, hit results, semantic output, or diagnostics transport.

### Existing owners preserved

- The existing conservative radius normalization, rounded fill/border replay,
  clipping, opacity, scrolling, PNG capture, software raster, point-hit,
  semantic/source order, and public `NativeBorderRadius` shape remain the
  downstream owners.
- No public schema, dependency, feature default, layout algorithm, or crate
  boundary changes are allowed. `native-engine` remains default-off inside
  `glass-browser`; the workspace remains exactly `glass-browser` and
  `glass-dev`.
- The slice remains local-resource-only, horizontal-tb, integer-pixel,
  fixed-cell, non-table, and explicitly non-browser-parity. Logical corner
  longhands, writing-mode-dependent corners, elliptical radii, and browser
  corner-rendering fidelity remain outside the claim.

## Tradeoffs

- Four private candidate arrays make shorthand/longhand precedence correct
  without changing the public four-corner value or duplicating geometry and
  paint code. They add a small amount of cascade state and a per-corner
  resolution pass.
- A corner longhand is represented through the existing radius declaration
  wrapper, with a private corner value for the selected stream. This keeps
  CSS-wide and `revert-layer` semantics aligned with the completed shorthand,
  at the cost of a private enum variant.
- Physical-only mapping avoids silently treating logical corner names as
  physical corners. Logical corner support will require an explicit
  writing-mode/direction contract and is not inferred from this slice.
- No dependency is added. All behavior remains inside `glass-browser`'s
  default-off native feature, preserving the two-crate build boundary.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Verification

- Format and run one locked native-feature `cargo check` before tests in an
  isolated task target.
- Run focused parser/cascade unit coverage for all four properties, all
  standalone CSS-wide keywords, case-insensitivity, source-order composition,
  invalid preservation, layer rollback, and inherited corner fallback.
- Run one focused integration fixture covering all corners through layout,
  box geometry, border/fill display commands, decoded raster/PNG capture,
  point-hit behavior, semantic/source order, and bounded diagnostics.
- Near completion run full native integration and feature-enabled browser
  library tests, strict affected-package Clippy, warning-denied rustdoc,
  paired locked `glass-dev` check/build, both package flows, repository
  static gates, workspace all-target/all-feature testing, and bounded cleanup.
- Record exact command results, counts, known warnings, remote-CI boundary,
  and cleanup evidence here before marking the task complete.

## Cleanup

Task-specific isolated targets and reports are safe to remove only after all
Cargo/Rust processes and open handles exit. Only exact paths created for this
task may be reclaimed; source, durable data, repository history, issue
snapshots, and unrelated workloads must remain untouched. Cleanup evidence
will record exact paths, measured bytes/files, process/open-handle checks,
post-delete absence, and filesystem free-space delta.

## Evidence

Implementation: `2b082ddf`; resolver/test-shape correction: `e6f3259d`.
The implementation adds private per-corner cascade streams for the four
physical circular radius longhands, preserves shorthand/longhand source order,
and reuses the existing resolved `NativeBorderRadius` through layout, display,
raster, PNG capture, point-hit, and semantic consumers. Public schemas,
dependencies, feature defaults, layout algorithms, and the two-crate boundary
are unchanged.

Documentation/certification: current product documentation, the native-engine
architecture record, plan history, analysis matrix, and issue #40 now describe
the completed physical corner-longhand contract and its exclusions. The final
documentation validator found 589 Markdown files, 83 current documents, 57
previous-version hits, 684 semantic-audit hits, and zero current-claim
failures.

Passed local gates:

- `cargo fmt --all -- --check` and `git diff --check`.
- Locked native-feature check, parser/cascade unit coverage, and the focused
  corner-longhand integration test in `/tmp/glass-175-focused`.
- Full native integration: 213 passed; native-feature browser library: 979
  passed, 1 ignored.
- Workspace all-target/all-feature tests: browser library 980 passed, 1
  ignored; native integration 213 passed; `glass-dev` library 365 passed;
  browser smoke 18 passed; development-runtime 4 passed; TUI PTY 15 passed;
  all other workspace targets passed with zero failures.
- Strict affected-package and workspace Clippy, warning-denied affected and
  workspace rustdoc, workspace all-target/all-feature check, and paired
  locked `glass-dev` check/build.
- Locked package flows for both crates; archives contained 196
  (`glass-browser`) and 69 (`glass-dev`) entries. The packaged dependency
  check resolved `glass-browser` exactly at `0.3.14`.
- `cargo deny check`, `cargo audit`, and offline locked all-target fuzz check.
  The configured audit policy reported four allowed warnings.
- Version sync, feature parity, release documentation, documentation
  coverage/depth, TUI shortcut inventory, reliability matrix, public read-only
  adapters, and Web IR corpus checks. The observed counts were 0 current-claim
  failures, 345 full-product MCP tools, 14 capabilities across 4 targets,
  93 guides/19 contracts, 15/63 TUI markers, 6 scenarios across 4 targets,
  5 adapters, and 8 fixtures/8 scenarios/11 categories.

Cleanup and issue-update evidence will be appended after the exact task target
and reports are reclaimed. Remote CI, push, release, tag, registry
publication, browser-parity, security-boundary, and promotion claims are not
made. The checkout remains local-only and issue #40 remains open for later
dependency-ordered slices.
