---
id: native-engine-039
scope: glass-browser/native-engine/fragment-anchor-scroll
status: done
depends-on: [native-engine-038]
---

# Native bounded fragment-target scrolling

## Objective

Make the existing local fragment/history path scroll to an exact visible
element `id` and restore bounded root scroll offsets when history entries are
traversed, without adding smooth scrolling, URL decoding policy, or a second
layout owner.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/history.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`

## Contract

For a same-resource navigation whose URL contains a non-empty raw fragment,
the native engine looks for one exact case-sensitive `id` attribute in the
current document. If that element has a visible layout box, its document-space
top becomes the root vertical scroll target, clamped to the current document's
maximum scroll offset. Missing, empty, hidden, or non-layout fragment targets
do not change the current scroll offset. Fragment matching remains raw and
bounded; percent-decoding, `name` anchors, text fragments, and duplicate-id
browser recovery are outside this slice.

Each `NativeHistoryEntry` retains its bounded root scroll offset. New
navigation entries record the committed offset, scroll actions update the
current entry's offset without adding history, and back/forward restores the
saved offset (clamped against a reparsed document's current layout bounds).
Same-document navigation still reuses the current document; different-resource
traversal still parses before changing the document or history cursor. A
fragment navigation with no matching target remains a successful URL/revision/
history transition but preserves the current scroll position.

The behavior is reachable through direct `NativeEngine::navigate`, explicit
Rust history traversal, and semantic local link activation through the existing
dispatcher action path. It does not add a transport-level history or scroll
operation. Smooth/animated, horizontal, nested, keyboard, snap, anchoring,
focus, and browser-parity scrolling remain unsupported.

## Tradeoffs

- Exact raw `id` matching is deterministic and avoids inventing a fragment
  decoding/base-URL policy before the native URL and origin workstream exists.
- Storing one bounded scroll point per history entry restores useful local
  navigation state without retaining full DOM snapshots, but mutable document
  control state is still not captured.
- Target-top alignment is simple and reproducible; it does not model browser
  block alignment, sticky elements, focus changes, smooth animation, or nested
  scroll containers.
- Missing targets preserve scroll rather than jumping to the top, avoiding a
  surprising state change for an unresolved local fragment.

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/history.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, feature, SDK, and plan docs

## Verification

- exact visible fragment `id` navigation scrolls to a bounded document-space
  target and clamps at the viewport maximum;
- missing/empty/hidden fragment targets preserve scroll while URL/revision/
  history still advance;
- explicit link activation reaches the same anchor-scroll path;
- scroll offsets update the active history entry and restore across same- and
  different-resource back/forward traversal;
- native integration and unit suites;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- native-feature doctests and documentation coverage, depth, release-truth,
  version-sync, and feature-parity validators; and
- `cargo fmt --all -- --check`, `git diff --check`, and one focused
  Conventional Commit before the next slice.

## Completion evidence

The implementation and synchronized docs are complete. The focused acceptance
test `fragment_targets_scroll_and_restore_bounded_history_offsets` passed,
covering semantic link activation, exact target-top clamping,
missing/hidden/duplicate target preservation, active-entry scroll updates,
same-document traversal, reparsed different-resource traversal, and bounded
restoration. The native integration suite passed 51/51 tests; the locked
all-target/all-feature `glass-browser` matrix passed 819 library tests (818
passed, 1 ignored), 51 native integration tests, all other integration tests,
examples, and workspace-facing targets. Native-feature doctests passed 4/4.

Strict Clippy passed for `--all-features` and `--no-default-features`. The
documentation coverage gate reports 453 Markdown files, 345 full-product MCP
tools, 17 examples, and 22 public modules; the depth gate reports 93 current
guides and 19 substantive contracts; release-truth reports zero current-claim
failures; version sync is 0.3.14; and feature parity reports 14 capabilities
across 4 targets. The workspace check passed with `RUST_MIN_STACK=8388608`:
819 `glass-browser` unit tests, 365 `glass-dev` unit tests, 4 development
integration tests, and 15 PTY tests. Formatting and whitespace checks passed.
