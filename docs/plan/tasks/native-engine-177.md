---
id: native-engine-177
scope: glass-browser/native-engine/cascade-radius-important-priority
status: complete
depends-on: [native-engine-176]
---

# Native bounded radius `!important` cascade priority

## Objective

Adds bounded author-origin `!important` priority to the complete native
`border-radius` family: the physical shorthand, four physical corner
longhands, and four horizontal-tb logical corner longhands. The declaration
value grammar and rounded geometry stay unchanged; only the private cascade
priority is extended so important radius declarations outrank normal radius
declarations and named-layer ordering is inverted for important declarations.

This is the smallest follow-up that closes the radius family's explicit
`!important` gap without pretending that the native engine has general CSS
origins, transitions, animations, or a second rendering path.

## Context

Native-engine-172 added CSS-wide values to the physical radius shorthand,
native-engine-175 added physical corner longhands, and native-engine-176 added
horizontal-tb logical corner longhands. All three currently use the same
15-named-layer plus unlayered author cascade, but the parser removes
`!important` without changing priority.

Normative references:

- <https://www.w3.org/TR/css-cascade-5/#importance>
- <https://www.w3.org/TR/css-cascade-5/#layer-order>

Read with:

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-172.md`
- `docs/plan/tasks/native-engine-175.md`
- `docs/plan/tasks/native-engine-176.md`

## Contract

### Supported priority

- The radius shorthand and all eight physical/logical corner longhands accept
  the existing bounded value or standalone CSS-wide/`revert-layer` form
  followed by a case-insensitive `!important` suffix.
- The suffix is recognized only as the terminal priority marker after optional
  whitespace. It is not part of the typed radius value, cannot be mixed into a
  value, and does not echo source text through diagnostics.
- Important declarations belong to the existing single author origin. User,
  user-agent, transition, animation, scoped-origin, and general CSS cascade
  origin behavior remain outside the native contract.

### Layer and source ordering

- Normal radius declarations retain the existing order: unlayered author
  declarations outrank later named layers, which outrank earlier named layers;
  selector specificity, rule order, declaration order, and inline precedence
  resolve ties within a layer.
- Important radius declarations outrank every normal radius declaration. Within
  the important partition, named-layer order is reversed: the earliest named
  layer wins, later named layers follow, and unlayered important declarations
  are the lowest-priority important bucket. Inline important radius values use
  the existing unlayered author bucket.
- `revert-layer !important` rolls back only the winning important layer and
  continues through lower-priority important candidates before normal
  candidates. Normal `revert-layer` keeps its current behavior.
- Physical and logical candidates still compete in the same mapped physical
  per-corner streams. A later declaration wins only when its importance,
  layer, specificity, origin bucket, and source order give it higher priority.

### Boundary

- Only radius declarations receive this priority behavior in this slice. Other
  currently supported properties retain their existing bounded parsing and
  cascade behavior; this is not a claim of generic `!important` support.
- No public schema, dependency, feature default, layout algorithm, raster
  algorithm, or crate boundary changes are allowed. `native-engine` remains
  default-off inside `glass-browser`, and the workspace remains exactly
  `glass-browser` plus `glass-dev`.
- Existing horizontal-tb, fixed-cell, integer-pixel, non-elliptical,
  local-resource, non-table, and non-browser-parity limits remain in force.

## Tradeoffs

- Radius candidates use a private doubled layer partition rather than changing
  the public cascade representation. This keeps the implementation local and
  avoids paying for a second priority dimension across every existing CSS
  property, at the cost of a small radius-only candidate-array expansion.
- The implementation models the current single author origin precisely enough
  for named layers and inline style, while leaving unimplemented origins and
  animation semantics typed and out of scope. Applying a false global fallback
  would be less trustworthy than the explicit radius-only boundary.
- `!important` is stripped with a case-insensitive terminal check so the
  accepted grammar is deterministic and does not alter diagnostics for raw
  stylesheet content. No dependency or public artifact field is needed.

## Path

- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, plan history, product
  capability docs, and issue #40 records

## Verification

- Format and run one locked native-feature `cargo check` before tests in an
  isolated task target.
- Run focused parser/cascade unit coverage for case-insensitive priority
  markers, normal-versus-important precedence, named-layer inversion,
  unlayered and inline important values, physical/logical mapped corners,
  source order, invalid preservation, CSS-wide values, and `revert-layer`.
- Run one focused integration fixture covering radius values through layout,
  rounded display commands, decoded raster/PNG capture, point-hit behavior,
  semantics/source order, and diagnostics.
- Near completion run full native integration and feature-enabled browser
  library tests, strict affected-package Clippy, warning-denied rustdoc,
  paired locked `glass-dev` check/build, both package flows, security/fuzz,
  repository static/workspace gates, and bounded cleanup.
- Record exact command results, counts, known warnings, remote-CI boundary,
  issue update, and cleanup evidence here before marking the task complete.

## Cleanup

Task-specific isolated targets and reports are safe to remove only after all
Cargo/Rust processes and open handles exit. Only exact paths created for this
task may be reclaimed; source, durable data, repository history, issue
snapshots, and unrelated workloads must remain untouched. Cleanup evidence
will record exact paths, measured bytes/files, process/open-handle checks,
post-delete absence, and filesystem free-space delta.

## Evidence

Implementation: `46f6499a`; design and documentation baseline:
`8fabb56d`. The implementation adds private doubled radius candidate
partitions, records terminal case-insensitive `!important` state for the
physical and horizontal-tb logical radius family, reverses named-layer order
only inside the important partition, and preserves the existing physical
projection and rounded consumers. No public schema, dependency, feature
default, layout algorithm, raster algorithm, or crate boundary changed.

Passed slice-local gates:

- `cargo fmt --all -- --check` and `git diff --check`.
- Locked native-feature `cargo check --tests` in the isolated target
  `/tmp/glass-177-focused`.
- Filtered priority coverage: 2 parser/cascade unit tests and 1 integration
  test passed; the integration exercised layout, rounded fill/border display
  commands, decoded raster/PNG capture, point hit testing, semantic/source
  order, and bounded unsupported diagnostics.
- Full native integration: 215 passed, 0 failed, 0 ignored.
- Native-feature `glass-browser` library: 984 passed, 1 ignored, 0 failed.
- Strict affected-package Clippy with `-D warnings` and warning-denied
  affected-package rustdoc passed.

The issue-level workspace all-target/all-feature, paired-crate, package,
security/fuzz, repository-static, remote-CI, and release gates remain
deferred until the dependency-ordered native-engine work reaches issue #40's
final validation boundary. This record makes no remote CI, push, release,
tag, registry-publication, browser-parity, or promotion claim.
