---
id: native-engine-042
scope: glass-browser/native-engine/legacy-name-fragment-targets
status: done
depends-on: [native-engine-041]
---

# Native bounded legacy `name` fragment targets

## Objective

Extend the existing decoded local fragment-target path to recognize one
bounded legacy `<a name="...">` anchor when no matching element `id` exists,
without adding text fragments, general URL parsing, or a second scrolling
owner.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

For a successful local navigation whose non-empty fragment has passed the
existing bounded UTF-8 percent-decoding step, target lookup first examines
exact, case-sensitive element `id` values. A unique matching `id` wins. A
duplicate matching `id` remains unresolved and must not fall through to a
`name` anchor. When no element `id` matches, lookup examines only `<a>`
elements with an exact, case-sensitive `name` value. A unique matching
`name` anchor becomes the target; duplicate matching anchors remain
unresolved. Empty names and names on non-`a` elements are ignored.

The selected target still passes the existing visible-layout-box gate before
changing root scroll. Missing, hidden, non-layout, duplicate, or otherwise
unresolved targets preserve the current scroll offset while the successful
URL/revision/history transition remains unchanged. Decoded spaces and UTF-8
values follow the 041 rules, and literal `+` remains a plus.

The behavior is reachable through direct navigation, explicit Rust history
traversal, and semantic local-link activation. Resource lookup, parse-before-
commit, bounded history scroll state, and failure-atomic unsupported
navigation remain unchanged. Text fragments, `name` matching on other
elements, duplicate-id recovery, smooth/nested/horizontal scrolling, focus
changes, and browser URL/scrolling parity remain outside this slice.

## Tradeoffs

- Restricting the legacy fallback to `<a name>` covers the historical anchor
  form without treating arbitrary `name` attributes as document targets.
- ID precedence and duplicate fail-closed behavior preserve deterministic
  target ownership instead of inventing browser recovery rules.
- Reusing the existing layout gate means a zero-size or hidden legacy anchor
  can update URL/history but cannot move the viewport.
- The feature adds no dependency and keeps percent-decoding, resource
  loading, and scrolling policies in their existing owners.

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, feature, SDK, README,
  experimental-capability, and plan docs

## Verification

- unique visible `<a name>` targets scroll after raw, spaced, and UTF-8
  percent-decoded navigation;
- exact ID precedence wins over a same-value legacy name anchor;
- duplicate IDs and duplicate names remain unresolved without scroll changes;
- hidden, empty, zero-layout, non-`a`, and case-mismatched names do not move
  the viewport;
- direct navigation, link activation, history traversal, and existing 041
  behavior remain green; and
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass.

## Completion evidence

Implemented in the local checkpoint that makes a unique exact `<a name>` a
decoded fragment-target fallback only when no matching `id` exists. The
checkpoint covers ID precedence, duplicate IDs and names, case-sensitive
matching, hidden/display-contents/non-`a` rejection, UTF-8 percent-decoded
names, semantic link activation, direct navigation, and history scroll
restoration.

Validation passed:

- focused legacy-name integration: 1 passed;
- full native integration: 54 passed;
- native-engine module units: 40 passed;
- locked all-target/all-feature browser matrix: 822 library tests passed, 1
  ignored, 54 native integration tests passed, and every remaining
  integration/example target passed;
- strict all-feature and no-default-feature Clippy passed;
- native-feature Rust doctests: 4 passed;
- complete workspace contract: 822 browser library tests passed with 1
  ignored, 365 `glass-dev` unit tests passed, 4 development-runtime tests
  passed, and 15 PTY tests passed;
- documentation coverage, depth, release-truth, version-sync, and
  feature-parity validators passed; and
- formatting and whitespace checks passed.

The checkpoint is committed locally and recorded on issue #40 before the next
slice. Remote CI remains pending because this branch has not been pushed.
