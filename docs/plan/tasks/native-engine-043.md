---
id: native-engine-043
scope: glass-browser/native-engine/text-fragment-targets
status: done
depends-on: [native-engine-042]
---

# Native bounded text-fragment targets

## Objective

Extend local fragment navigation with a bounded `#:~:text=` target form that
scrolls to the first matching visible text run, while keeping the existing
decoded ID/legacy-name target path, history owner, and local-only resource
boundary intact.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/config.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

After the existing raw-fragment split, a fragment beginning exactly with
`#:~:text=` is treated as a bounded text-fragment request. Its payload has
either one non-empty percent-decoded UTF-8 `start` term or two
percent-decoded UTF-8 terms separated by one raw comma (`start,end`). A
percent-encoded comma belongs to a term. Matching is exact and
case-sensitive against the first visible, non-truncated `NativeTextLayout`
run in document order. With `start,end`, both terms must occur in that same
run in order; the run's document-space top becomes the root scroll target.

Text fragments do not become semantic nodes, highlights, or evidence. The
existing layout visibility/zero-box gate, root scroll clamping, successful
URL/revision/history transition, per-entry scroll state, link activation, and
history traversal behavior remain unchanged. Missing, empty, malformed,
invalid-UTF-8, hidden, non-layout, out-of-order, or unsupported text-fragment
forms preserve the current offset.

Prefix/suffix text-fragment syntax, multiple text directives, cross-run ranges,
Unicode normalization, browser text-fragment matching parity, and general URL
decoding remain outside this slice. ID lookup and then unique legacy `<a
name>` lookup remain the target path for all non-text fragments.

## Tradeoffs

- Matching one rendered text run gives deterministic root scrolling without
  adding a second text projection or a full range/highlight model.
- Parsing the raw comma before per-term decoding lets encoded commas remain
  part of a target while keeping the accepted grammar small.
- First-match behavior is useful for local fixtures, but duplicate text and
  browser prefix/suffix disambiguation are intentionally not claimed.
- The implementation reuses the existing layout snapshot and adds no
  dependency or resource-loading capability.

## Path

- `crates/glass-browser/src/browser/native_engine/config.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, feature, SDK, README,
  experimental-capability, and plan docs

## Verification

- one-term and same-run `start,end` text fragments scroll through direct
  navigation and semantic local-link activation;
- percent-decoded spaces, UTF-8, and encoded commas match without form-style
  plus conversion;
- missing, duplicate/first-match, hidden, zero-layout, malformed,
  invalid-UTF-8, out-of-order, prefix/suffix, and cross-run cases fail closed
  to the existing scroll offset;
- ID and legacy-name fragments, history traversal, and resource failure
  atomicity remain green; and
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass.

## Completion evidence

Implemented in the focused `feat(native-engine): support bounded text
fragments` checkpoint and recorded on issue #40. The focused DOM and
integration tests, full native integration suite (55 tests), native module
unit suite (42 tests), strict all-feature/no-default Clippy gates, all-feature
doctests (4 tests), formatting, and repository documentation validators pass.
