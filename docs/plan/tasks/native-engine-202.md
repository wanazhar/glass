---
id: native-engine-202
scope: glass-browser/native-engine/inherited-text-decoration-skip-ink-css-wide-resets
status: complete
depends-on: [native-engine-201]
---

# Native inherited text-decoration-skip-ink CSS-wide resets

## Objective

Extend the existing bounded inherited `text-decoration-skip-ink` owner with
standalone CSS-wide reset keywords without changing same-run glyph intersection
behavior, line ownership, raster geometry, generic cascade machinery, or the
two-crate boundary.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-201.md`

## Contract

- Inherited `text-decoration-skip-ink` accepts standalone, case-insensitive
  `inherit`, `initial`, `unset`, and one-author-origin `revert`, in addition to
  the existing `revert-layer` and finite `auto|none` values.
- `inherit` and `unset` resolve to the computed parent value. In the current
  one-author-origin engine, `revert` uses the same parent fallback because no
  lower author origin exists. `revert-layer` remains the only form that walks
  lower named-layer candidates.
- `initial` resolves to the existing finite `auto` root fallback. A winning
  CSS-wide keyword is terminal for this property; invalid later declarations
  preserve the preceding valid declaration, and existing important/source-order
  behavior remains unchanged.
- Mixed reset tokens, additional values, fragment-level ink metrics, font
  shaping, additional origins, and generic CSS-wide machinery remain outside
  this bounded contract.

## Boundary and tradeoffs

- Reuse the existing private declaration/candidate stream and inherited
  resolver. No public computed-style field, display-command field, dependency,
  or crate is introduced.
- The resolved finite skip-ink mode continues through the existing decoration
  line owner, same-run glyph intersection test, display-list, capture, and
  fixed-cell raster consumers. This slice does not change text metrics,
  underline/overline origins, line-through behavior, thickness, style, color,
  skip-spaces, or layout.
- The explicit `auto` initial fallback preserves the current root behavior;
  browser-specific ink heuristics and font-derived intersection geometry are
  deliberately not implied by accepting CSS-wide reset keywords.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive standalone forms, root and
  parent fallback, terminal reset behavior, invalid-later preservation,
  `!important`, source order, and the `revert-layer` distinction.
- Run one public fixture through inherited skip-ink modes, same-run glyph
  intersection, line-through preservation, display-list, decoded raster/PNG,
  and diagnostic consumers.
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

- Implementation commit: `65e3a76d`; design commit: `d435032c`.
- Locked native-feature check passed before tests.
- Focused parser/cascade coverage passed: 3 unit tests; the public fixture
  passed through display-list, decoded raster/PNG, and diagnostics.
- Full native integration passed: 240/240. The native-feature library passed
  1,022 tests with 1 ignored test.
- Workspace all-target/all-feature tests passed: browser library 1,023 passed
  with 1 ignored; native integration 240 passed; browser smoke 18 passed;
  daemon recovery 1 passed; glass-dev 365 passed; development runtime 4
  passed; PTY 15 passed; remaining targets reported no failures. Workspace
  doctests passed: 4 browser and 1 dev.
- Paired browser/dev builds, strict Clippy, warning-denied rustdoc, fuzz
  checks, cargo-deny, cargo-audit, formatting, package assembly, and exact
  packaged dependency verification passed locally. Packages were 196 files /
  6.0 MiB for `glass-browser` and 69 files / 2.6 MiB for `glass-dev`; the
  packaged dev archive resolves `glass-browser` exactly at `0.3.14`.
- The direct registry-backed dev verification remains blocked by the immutable
  public `glass-browser 0.3.14` API surface; the canonical local patched /
  no-verify package route passes. Known non-blocking repository warnings remain
  the duplicate `winnow` deny warning and four cargo-audit warnings allowed by
  policy (unmaintained `bincode`/`yaml-rust`, the `lru` advisory, and yanked
  `chacha20`).
- Static documentation audits passed: release truth reported 616 Markdown
  documents, 83 current-version documents, 59 previous-version hits, 728
  semantic hits, and zero current-claim failures; coverage reported 345 full
  product MCP tools (100 browser-only), 17 examples, and 22 public modules;
  depth reported 93 current guides and 19 substantive contracts; parity
  reported 14 capabilities across 4 targets; TUI reported 15 implementation
  help keys and 63 documentation markers; reliability reported 6 scenarios
  across 4 targets; read-only adapters reported 5; Web IR reported 8 fixtures,
  8 scenarios, and 11 categories.
- Bounded cleanup is performed only after Cargo/Rust processes and open handles
  are checked. Remote CI, push, release, tag, registry publication,
  browser-parity, security-boundary certification, and promotion remain outside
  this local task.
