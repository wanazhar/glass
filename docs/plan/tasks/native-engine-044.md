---
id: native-engine-044
scope: glass-browser/native-engine/text-fragment-affixes
status: complete
depends-on: [native-engine-043]
---

# Native bounded text-fragment prefix/suffix affixes

## Objective

Extend the bounded `#:~:text=` navigation form with exact prefix and suffix
affixes around the existing same-run `start[,end]` matcher, without adding a
second text tree, highlight renderer, or general URL-decoding policy.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/config.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

After the existing raw-fragment split, a fragment beginning exactly with
`#:~:text=` accepts the following bounded payload forms, using raw commas as
separators before decoding each term:

- `start`
- `start,end`
- `prefix-,start`
- `start,-suffix`
- `prefix-,start,-suffix`
- `prefix-,start,end`
- `start,end,-suffix`
- `prefix-,start,end,-suffix`

The literal trailing `-` on a first term marks a prefix and the literal
leading `-` on a final term marks a suffix. The marker is removed before
bounded UTF-8 percent decoding; encoded hyphens remain term data. Every
decoded term must be non-empty. `start` and optional `end` match exactly and
case-sensitively in the first visible, non-truncated `NativeTextLayout` run in
document order. Prefix must end immediately before `start`; suffix must begin
immediately after `end`, or after `start` when `end` is absent. A range remains
inside one run and the end must follow the start. The run's document-space top
becomes the existing clamped root-scroll target.

Text fragments remain navigation metadata rather than semantic nodes or
highlights. The existing layout visibility/zero-box gate, root-scroll clamping,
successful URL/revision/history transition, per-entry scroll state, link
activation, history traversal, and resource failure atomicity remain unchanged.
Empty, malformed, invalid-UTF-8, ambiguous, hidden, non-layout, out-of-order,
cross-run, or unsupported forms preserve the current offset.

Multiple directives, highlight rendering, Unicode normalization, general URL
decoding, cross-run ranges, browser text-fragment matching parity, and the
remaining general browser-engine phases remain outside this slice.

## Tradeoffs

- Raw-affix recognition keeps encoded hyphens in user terms and avoids
  confusing percent-decoding with grammar parsing.
- Exact byte adjacency is deterministic and does not synthesize whitespace or
  cross-node text, but it intentionally misses browser range-normalization
  behavior.
- Reusing the existing layout snapshot keeps the slice dependency-free and
  preserves the native engine's one-owner document/layout model.

## Path

- `crates/glass-browser/src/browser/native_engine/config.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, and plan docs

## Verification

- all seven accepted affix forms decode and match through direct navigation
  and semantic local-link activation where applicable;
- prefix/suffix markers are removed before per-term decoding, while encoded
  commas/hyphens and UTF-8 remain term data;
- exact adjacency, first visible run ordering, same-run ordered ranges, and
  history/scroll behavior are covered;
- empty, malformed, invalid-UTF-8, ambiguous, hidden, non-layout, out-of-order,
  cross-run, and unsupported forms fail closed to the existing scroll offset;
- native integration/unit, strict lint, formatting, whitespace, and the
  documentation validators pass.

## Completion evidence

Implementation and focused validation are complete locally. The code, docs,
and integration coverage are committed as a focused Conventional Commit and
the evidence is recorded on issue #40. No later native-engine slice is active
in this checkout.

Validation evidence:

- `cargo fmt --all -- --check` passed;
- the focused native-engine unit run passed with 42 tests;
- the focused affix integration test passed;
- the full native-engine integration suite passed with 56 tests;
- strict Clippy passed with all features and with no default features;
- documentation depth, release-documentation, version-sync, feature-parity,
  and binary documentation-coverage validators passed (458 Markdown files,
  345 full-product MCP tools, 17 examples, and 22 public modules).
