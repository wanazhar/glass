---
id: native-engine-205
scope: glass-browser/native-engine/local-gap-css-wide-resets
status: complete
depends-on: [native-engine-204]
---

# Native local gap CSS-wide resets

## Objective

Extend the existing bounded local `gap`, `row-gap`, and `column-gap` owners
with standalone CSS-wide reset keywords without changing flex/grid placement,
shorthand/longhand source order, named-layer rollback, display artifacts, or
the two-crate boundary.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-204.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`

## Contract

- The bounded local `gap`, `row-gap`, and `column-gap` declarations accept
  standalone, case-insensitive `initial`, `unset`, and one-author-origin
  `revert`, in addition to the existing finite non-negative pixel values and
  `revert-layer`.
- `initial`, `unset`, and `revert` resolve to the current local zero-gap
  fallback. A winning reset candidate is terminal for that axis; invalid later
  declarations preserve the preceding valid declaration, and existing
  shorthand/longhand, important, source-order, and named-layer behavior remain
  unchanged.
- `gap` reset forms write both existing row and column candidate streams;
  `row-gap` and `column-gap` reset forms affect only their respective axis.
  The resolved axes continue through the existing flex placement, normal-flow,
  display-list, raster, hit, and semantic consumers.
- `inherit`, percentages, negative/fractional lengths, intrinsic values,
  grid-specific track sizing, additional origins, and generic CSS-wide
  machinery remain outside this local bounded contract. In particular, this
  slice does not add parent-gap propagation to the inherited-style carrier.

## Boundary and tradeoffs

- Reuse the private gap shorthand/longhand candidate arrays and existing
  zero-value resolver. No public computed-style field, display-command field,
  dependency, or crate is introduced.
- Keeping `inherit` unsupported avoids changing the DOM-to-style inheritance
  contract for a non-inherited layout property; callers receive the existing
  typed unsupported-value diagnostic rather than a misleading parent value.
- Resetting to zero preserves the current local omission fallback and keeps
  flex placement deterministic, but does not imply browser-wide grid or gap
  conformance.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive reset forms, zero fallback,
  terminal reset behavior, invalid-later preservation, shorthand/longhand
  axis projection, `!important`, source order, and `revert-layer`.
- Run one public fixture through row/column placement, display-list, fixed-cell
  raster/PNG, point-hit/semantic consumers, and typed unsupported-value
  diagnostics for excluded `inherit`/unsupported lengths.
- Near issue completion retain the full native integration, feature-library,
  strict lint, rustdoc, paired two-crate, package, security/fuzz, static
  documentation, workspace all-target/all-feature, clean-install, issue-sync,
  and bounded cleanup gates.

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

- Implementation commit: `46128c2e`; design commit: `8852adee`.
- The locked native-feature test-target check passed before tests. Focused
  parser/cascade coverage passed 10 gap-related unit tests, and the public
  fixture passed through case-insensitive reset parsing, zero-gap fallback,
  shorthand/longhand axis projection, important/source order,
  invalid-later preservation, flex placement, point hit testing, display-list,
  fixed-cell raster/PNG, and typed unsupported-value diagnostics for excluded
  `inherit`.
- Full native integration passed: 243/243. The native-feature library passed
  1,025 tests with 1 ignored test.
- Paired browser/dev builds, strict Clippy, warning-denied rustdoc, workspace
  all-target/all-feature checking, formatting, fuzz checks, cargo-deny,
  cargo-audit, package assembly, and the exact packaged dependency check
  passed locally. Workspace all-target/all-feature tests passed: browser
  library 1,026 passed with 1 ignored; native integration 243 passed; browser
  smoke 18 passed; daemon recovery 1 passed; `glass-dev` 365 passed;
  development runtime 4 passed; PTY 15 passed; remaining targets reported no
  failures. Workspace doctests passed: 4 browser and 1 dev.
- Packages were 196 files / 6.0 MiB for `glass-browser` and 69 files /
  2.6 MiB for `glass-dev`; the packaged dev archive resolves
  `glass-browser` exactly at `0.3.14`. Direct registry-backed dev
  verification remains blocked by the immutable public `glass-browser 0.3.14`
  API surface; the canonical local patched/no-verify package route passes.
- Known non-blocking repository warnings remain the duplicate `winnow` deny
  warning and four cargo-audit warnings allowed by policy (unmaintained
  `bincode`/`yaml-rust`, the `lru` advisory, and yanked `chacha20`).
- Static documentation audits passed: release truth reported 619 Markdown
  documents, 83 current-version documents, 59 previous-version hits, 737
  semantic hits, and zero current-claim failures; coverage reported 345
  full-product MCP tools (100 browser-only), 17 examples, and 22 public
  modules; depth reported 93 current guides and 19 substantive contracts;
  parity reported 14 capabilities across 4 targets; TUI reported 15
  implementation help keys and 63 documentation markers; reliability
  reported 6 scenarios across 4 targets; read-only adapters reported 5; Web
  IR reported 8 fixtures, 8 scenarios, and 11 categories; version remained
  synchronized at `0.3.14`.
- The preceding clean-install transition gate remains the latest install
  evidence because this slice changed only native CSS parsing/cascade and
  synchronized documentation, not package metadata or dependencies. Exact
  `/tmp/glass-200-focused` cleanup remains a bounded post-certification action
  after Cargo/Rust process and open-handle checks; no process is being
  terminated.
- Remote CI, push, release, tag, registry publication, browser-parity,
  security-boundary certification, and promotion remain outside this local
  task.
