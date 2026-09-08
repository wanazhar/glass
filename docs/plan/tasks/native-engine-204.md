---
id: native-engine-204
scope: glass-browser/native-engine/inherited-text-underline-offset-css-wide-resets
status: complete
depends-on: [native-engine-203]
---

# Native inherited text-underline-offset CSS-wide resets

## Objective

Extend the existing bounded inherited `text-underline-offset` owner with
standalone CSS-wide reset keywords without changing underline-only geometry,
shared text commands, fixed-cell raster behavior, generic cascade machinery,
or the two-crate boundary.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-203.md`
- `crates/glass-browser/src/browser/native_engine/css.rs`

## Contract

- The bounded inherited `text-underline-offset` owner accepts standalone,
  case-insensitive `inherit`, `initial`, `unset`, and one-author-origin
  `revert`, in addition to the existing `revert-layer` and signed finite
  `-4px..=4px` values.
- `inherit` and `unset` resolve to the computed parent offset. In the current
  one-author-origin engine, `revert` uses the same parent fallback because no
  lower author origin exists. `revert-layer` remains the only form that walks
  lower named-layer candidates.
- `initial` resolves to the existing finite `0px` root fallback. A winning
  CSS-wide keyword is terminal for this property; invalid later declarations
  preserve the preceding valid declaration, and existing important/source-order
  behavior remains unchanged.
- The reset forms apply through the existing underline-only translation owner.
  Overline and line-through origins, shared text metrics, clipping, capture,
  and fixed-cell raster behavior remain unchanged.
- Mixed reset tokens, `auto`, percentages, fractional or font-derived values,
  dimensions outside the bounded range, additional origins, and generic
  CSS-wide machinery remain outside this bounded contract.

## Boundary and tradeoffs

- Reuse the existing private declaration/candidate stream and inherited
  resolver. No public computed-style field, display-command field,
  dependency, or crate is introduced.
- Keeping the current signed integer range makes reset behavior deterministic
  and preserves the existing replay contract, but it does not claim browser
  support for CSS `auto`, percentages, font-relative values, or arbitrary
  lengths.
- `initial` remains the current engine's zero-pixel fallback rather than
  introducing a new layout or baseline calculation. This keeps the slice
  focused, while browser-derived underline metrics and decoration-origin
  propagation remain future work.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive standalone forms, root and
  parent fallback, terminal reset behavior, invalid-later preservation,
  `!important`, source order, and the `revert-layer` distinction.
- Run one public fixture through inherited signed offsets, underline-only
  movement with overline/line-through preservation, display-list, decoded
  raster/PNG, and typed unsupported-value diagnostics.
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

- Implementation commit: `c82773e2`; design commit: `9473832f`.
- The locked native-feature test-target check passed before tests. Focused
  parser/cascade coverage passed 4 decoration-related unit tests, and the
  public fixture passed through case-insensitive reset parsing, parent/root
  fallback, terminal reset behavior, invalid-later preservation,
  important/source order, underline-only movement with overline/line-through
  preservation, display-list, fixed-cell raster/PNG, and typed unsupported-
  value diagnostics.
- Full native integration passed: 242/242. The native-feature library passed
  1,024 tests with 1 ignored test.
- Paired browser/dev builds, strict Clippy, warning-denied rustdoc, workspace
  all-target/all-feature checking, formatting, fuzz checks, cargo-deny,
  cargo-audit, package assembly, and the exact packaged dependency check
  passed locally. Workspace all-target/all-feature tests passed: browser
  library 1,025 passed with 1 ignored; native integration 242 passed; browser
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
- Static documentation audits passed: release truth reported 618 Markdown
  documents, 83 current-version documents, 59 previous-version hits, 734
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
