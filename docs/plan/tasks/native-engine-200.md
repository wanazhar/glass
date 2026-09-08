---
id: native-engine-200
scope: glass-browser/native-engine/inherited-text-decoration-style-css-wide-resets
status: complete
depends-on: [native-engine-199]
---

# Native inherited text-decoration-style CSS-wide resets

## Objective

Extend the existing bounded inherited `text-decoration-style` owner with
standalone CSS-wide reset keywords without adding generic cascade machinery,
new computed-style state, or a new crate.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-199.md`

## Contract

- Inherited `text-decoration-style` accepts standalone, case-insensitive
  `inherit`, `initial`, `unset`, and one-author-origin `revert`, in addition to
  the existing `revert-layer` and finite `solid|dashed|dotted|double|wavy`
  values.
- `inherit` and `unset` resolve to the computed parent style. In the current
  one-author-origin engine, `revert` uses the same parent fallback because no
  lower author origin exists. `revert-layer` remains the only form that walks
  lower named-layer candidates.
- `initial` resolves to the existing finite `solid` root fallback.
- A winning CSS-wide keyword is terminal for this property. Invalid later
  declarations preserve the preceding valid declaration, and existing
  important/source-order behavior plus the decoration display-list/raster
  consumers remain unchanged.
- Mixed reset tokens, unsupported decoration values, additional origins,
  browser font metrics, and generic CSS-wide machinery remain outside this
  bounded contract.

## Boundary and tradeoffs

- Reuse `InheritedTextDeclaration<NativeTextDecorationStyle>` and the existing
  bounded inherited resolver rather than introducing another public enum or
  computed-style field.
- The finite decoration style continues through the existing text decoration
  command and fixed-cell raster owner. This slice does not change decoration
  geometry, thickness, offset, skip behavior, line propagation, or paint color.
- `text-decoration-style` is intentionally isolated from the still-unsupported
  CSS-wide forms on other decoration properties so parser, cascade, and artifact
  ownership remain easy to audit.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser/cascade coverage on case-insensitive standalone forms, root and
  parent fallback, terminal reset behavior, invalid-later preservation,
  `!important`, source order, and the `revert-layer` distinction.
- Run one public decoration fixture through style inheritance, display-list,
  raster/PNG, and unchanged semantic/diagnostic consumers.
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

Implementation is committed locally as `62525ec4` (`feat(native-engine):
support decoration style resets`); the task design is recorded in `9eafebc9`.
The locked native-feature check passed in the isolated task target
`/tmp/glass-200-focused`. Focused parser/cascade coverage passed for the
standalone case-insensitive `inherit`, `initial`, `unset`, and one-author-origin
`revert` forms, and the public integration fixture passed through inherited
style resolution, display-list commands, fixed-cell raster output, and the
unsupported-value diagnostic path. Full native integration passed with 238/238
tests; the full native-feature library passed with 1,020 tests and one ignored
test under the configured large test-thread stack.

The paired browser and dev binaries built successfully. Strict two-crate
Clippy with `-D warnings`, warning-denied rustdoc, workspace all-target/all-
feature checking and tests, workspace doctests, fuzz-manifest fetch and
offline all-target checking,
formatting, `cargo-deny`, and `cargo-audit` passed. `cargo-deny` reported only
the existing duplicate `winnow` warning; `cargo-audit` reported the four
previously allowed dependency warnings (two unmaintained crates, the current
`lru` advisory, and the yanked `chacha20` release).

Static documentation gates passed with 614 Markdown documents (83 current,
59 previous-version hits, 722 semantic audit hits, and 0 current-claim
failures), 345 full-product MCP tools (100 browser-only), 17 examples, 22
public modules, 93 current guides, 19 substantive contracts, 14 capabilities
across 4 targets, 15 implementation help keys, 63 documentation markers,
Web IR 8/8/11 corpus coverage, and version `0.3.14`.

The locked no-verify package path produced the 196-file, 6.0 MiB
`glass-browser 0.3.14` archive and the 69-file, 2.6 MiB `glass-dev 0.3.14`
archive. The dev archive's dependency was checked at exactly
`glass-browser 0.3.14` using the documented local crates.io patch. Direct
registry-backed verification remains blocked by the immutable public
`glass-browser 0.3.14` archive lacking the current runtime API; no upload was
attempted. The prior task's clean-install transition gate remains the latest
install evidence because this slice changed only native CSS parsing/cascade
and did not change package metadata or dependencies.

Remote CI, push, release, tag, registry publication, browser parity,
security-boundary certification, and promotion remain outside this local
task. Exact temporary-target cleanup and issue synchronization follow the
documentation and final-gate pass.
