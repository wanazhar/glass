---
id: native-engine-199
scope: glass-browser/native-engine/inherited-alignment-css-wide-resets
status: complete
depends-on: [native-engine-198]
---

# Native inherited alignment and direction CSS-wide resets

## Objective

Extend the existing bounded inherited text-alignment and direction owners with
standalone CSS-wide keywords without adding a generic cascade engine or new
computed-style/artifact state.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-198.md`

## Contract

- Inherited `text-align`, `text-align-last`, `text-justify`, and `direction`
  accept standalone, case-insensitive `inherit`, `initial`, `unset`, and
  one-author-origin `revert`, in addition to the existing `revert-layer`.
- `inherit` and `unset` resolve to the computed parent value. In the current
  one-author-origin engine, `revert` uses the same parent fallback because no
  lower author origin exists. `revert-layer` remains the only form that walks
  lower named-layer candidates.
- `initial` resolves to the existing finite root defaults: left alignment,
  auto last-line alignment, auto text justification, and `ltr` direction.
- A winning CSS-wide keyword is terminal for its property. Invalid later
  declarations preserve the preceding valid declaration, and existing
  specificity, source order, inline-important, important-over-normal, and
  logical `ltr`/`rtl` projection behavior remain unchanged.
- Mixed reset tokens, unsupported alignment values, vertical writing modes,
  additional origins, and generic CSS-wide machinery remain outside the
  bounded contract.

## Boundary and tradeoffs

- The four private declaration types converge on the existing inherited text
  fallback resolver while public finite enums and layout/display-list/raster
  consumers remain unchanged.
- `direction` continues to feed logical border projection and flex-axis
  mapping through its existing computed owner. The slice does not add browser
  bidi, writing-mode, shaping, or full CSS direction conformance.
- Keeping `text-align-last: auto` and `text-justify: auto` as the finite
  initial values preserves the existing line-placement fallback instead of
  inventing a second used-value representation.

## Verification

- Run one locked native-feature test-target `cargo check` in an isolated task
  target before tests.
- Focus parser coverage on case-insensitive standalone forms and mixed-token
  rejection for all four owners.
- Focus cascade coverage on parent/root fallback, terminal reset behavior,
  explicit `inherit`, invalid-later preservation, `!important`, source order,
  and the `revert-layer` distinction.
- Run one public integration fixture through alignment/direction layout,
  display-list/raster/PNG, point-hit, semantic, and diagnostic consumers.
- Near issue completion retain full native integration, feature library,
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

Implementation is committed locally as `4771f352` (`feat(native-engine):
support alignment resets`); the task design is recorded in `4b02b41b`.
The locked native-feature check passed in the isolated task target
`/tmp/glass-199-focused`. Focused cascade coverage passed for the inherited
alignment reset resolver, parser coverage passed for the alignment and
direction declaration families, and the public integration fixture passed
through layout, display-list, raster/PNG, point-hit, semantic, and diagnostic
consumers. Full native integration passed with 237/237 tests. Formatting and
the diff check passed after the fixture correction that isolates inherited
`text-align`/`direction` placement from final-line `text-align-last` behavior.

The issue-level local certification batch now passes. The full native-feature
library passed with 1,020 tests and one ignored test under
`RUST_MIN_STACK=8388608`; both crate binaries built, strict two-crate Clippy
and warning-denied rustdoc passed, and the workspace all-target/all-feature
test gate passed (1,021 browser-library tests plus the glass-dev suite,
development-runtime tests, and 15 PTY tests). Workspace doctests passed with
four `glass-browser` and one `glass-dev` doctests. The fuzz manifest fetched
and checked offline, `cargo-deny` passed all configured checks, and
`cargo-audit` passed with four allowed dependency warnings (two unmaintained
crates, the current `lru` advisory, and the yanked `chacha20` release).

Both 0.3.14 archives were created and the packaged `glass-dev` dependency
was verified at exactly `glass-browser 0.3.14`. Direct registry-backed
verification of the dev archive remains blocked by the immutable crates.io
`glass-browser 0.3.14` package lacking the current runtime API; the canonical
local release path's explicit local patch/no-verify verification passed. The
canonical `GLASS_PREVIOUS_VERSION=0.3.13 scripts/smoke-clean-install.sh`
then passed current core/full installs, ownership transitions, browser-only
replacement, and the 0.3.13-to-0.3.14 transition.

Static gates passed with 613 Markdown documents (83 current, 59
previous-version hits, 719 semantic audit hits, 0 current-claim failures),
345 full-product MCP tools
(100 browser-only), 17 examples, 22 public modules, 93 current guides, 19
substantive contracts, 14 capabilities across 4 targets, 15 implementation
help keys, 63 documentation markers, Web IR 8/8/11 corpus coverage, and
version `0.3.14`. Formatting and diff checks passed. The focused target and
temporary clean-install roots are still retained until final process/open-
handle checks and issue synchronization complete. Remote CI, push, release,
tag, registry publication, browser parity, security-boundary certification,
and promotion remain outside this local task.
