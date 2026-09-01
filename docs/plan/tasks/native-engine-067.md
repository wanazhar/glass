---
id: native-engine-067
scope: glass-browser/native-engine/flex-item-order
status: complete
depends-on: [native-engine-066]
---

# Native engine 067: bounded flex-item order

## Objective

Add a bounded, deterministic `order` property to the existing eligible
single-row flex layout. This slice changes visual flex-item order while
preserving source DOM order for semantic projection and locator identity.
It must compose with the existing fixed widths, margins, `gap`, and
`justify-content` owners without adding a third crate or a dependency.

## Contract

The native CSS grammar accepts `order` only as a non-inherited signed decimal
integer in the inclusive range `-1024..=1024`. The default used value is `0`.
Whitespace around the value is allowed by the existing declaration parser;
decimal fractions, exponent notation, CSS-wide keywords, values outside the
bound, and malformed signs are unsupported and retain the inherited/default
fallback of `0`. The property participates in the existing selector, cascade,
and inline precedence rules, but it never inherits to descendants.

For a block-level `display:flex` container that passes the existing eligibility
gate:

- visible direct element children are sorted by ascending used `order`;
- equal-order items retain source order through an explicit stable tie key;
- hidden and `display:none` children remain absent and do not consume a sort,
  gap, or justification position;
- the sorted items then use the existing fixed-width row geometry, margins,
  non-negative pixel gap, and `justify-content` free-space distribution;
- the sorted layout order is shared by child boxes, descendant layout,
  display-list paint order, viewport projection, root overflow, and point hit
  testing;
- semantic DOM traversal, source text, node references, and accessibility
  source order remain document order; `order` is a visual flex-item contract,
  not a semantic or keyboard-navigation reorder;
- an ineligible flex container continues to use normal-flow fallback and
  ignores all child `order` values without dropping meaningful text,
  `display:contents`, or visible `<br>` content.

The default `order:0` path must be coordinate- and paint-equivalent to the
current 066 row behavior. Negative order values may move an item ahead of
source-order siblings, but no item may receive a negative coordinate. Overflow
rows retain the existing zero-leading-offset and root-scroll behavior.

## Explicit exclusions

This slice does not add flex growth, shrinkage, basis, wrapping, reverse or
column direction, cross-axis alignment, anonymous text-item sorting,
`display:contents` flattening, `space-around`, `space-evenly`, nested
scrolling, stacking-context parity, keyboard/tab-order changes, accessibility
reordering, or browser Flexbox conformance. It does not change non-flex
normal-flow ordering or the stable package topology.

## Tradeoffs and risks

Sorting eligible items adds bounded `O(n log n)` work and carries the visual
order through paint/hit-test code, which is the smallest coherent behavior for
this property. Keeping semantic order unchanged avoids silently changing
locators and document evidence, but it intentionally differs from any future
keyboard-navigation or accessibility-order semantics. The integer bound makes
parsing and comparisons deterministic and prevents pathological values, while
rejecting valid browser values outside the bound; that limitation must remain
visible in diagnostics and documentation. Because overlapping/positioned
stacking is outside the engine, this slice does not claim full CSS painting
order parity.

## Implementation boundary

Expected implementation ownership:

- `native_engine::css`: `NativeOrderValue`/bounded parser, non-inherited
  computed field, cascade, inline declarations, and supported-property
  diagnostics;
- `native_engine::layout`: stable `(order, source_index)` sorting before the
  existing row preflight, with no change to normal-flow fallback;
- native integration tests: parser/cascade, negative and positive values,
  stable ties, hidden-item filtering, composition with gap/justification,
  overflow/hit-test coordinates, and fallback preservation;
- architecture/analysis/plan records: current capability and explicit
  exclusions only after implementation evidence is complete.

## Acceptance evidence

- [x] Design record is committed before implementation.
- [x] CSS unit tests cover accepted bounds, rejected syntax, cascade, inline
  precedence, defaulting, and non-inheritance.
- [x] Integration tests prove negative/positive sorting, stable ties, hidden
  filtering, gap/justification composition, overflow preservation, and
  normal-flow fallback.
- [x] Layout boxes, descendant geometry, display-list order, hit testing, and
  root overflow consume the same sorted coordinates.
- [x] Native integration suite, strict lint, formatting, whitespace, and
  documentation validators pass.
- [x] Implementation and documentation checkpoints are committed locally and
  issue #40 is updated; remote CI is not claimed until this branch is pushed.
- [x] Exact regenerable Cargo outputs are reclaimed after all validation,
  without terminating long-lived Glass processes.

## Completion evidence

- Design checkpoint: `09f3b00` (`docs(native-engine): define flex item order slice`).
- Implementation checkpoint: `a713b6e` (`feat(native-engine): add bounded flex item order`).
- CSS parser/cascade tests: 2 passed, 0 failed.
- Focused flex-item-order integration tests: 2 passed, 0 failed.
- Full native integration suite: 85 passed, 0 failed.
- Feature-enabled strict Clippy passed in 10m33s; no-default strict Clippy
  passed in 5m01s.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- Runtime-backed documentation coverage, release-truth, documentation-depth,
  version, feature-parity, and the other documentation validators passed with
  zero current-claim failures after the current-source update.
- Exact regenerable Cargo outputs were reclaimed after validation; no
  long-lived Glass process was terminated.
- Remote CI remains pending because this branch is local-only.
