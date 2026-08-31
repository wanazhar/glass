---
id: native-engine-037
scope: glass-browser/native-engine/local-navigation-history
status: done
depends-on: [native-engine-036]
---

# Native bounded same-document navigation and history traversal

## Objective

Extend the local native navigation boundary with fragment-aware
same-document navigation and bounded back/forward traversal, while preserving
the real dispatcher navigation path, revision safety, parse-before-commit
behavior, and the two-crate/default-off architecture.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/history.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`

## Contract

The existing local URL forms remain the only resource forms: `about:blank`,
UTF-8 percent-decoded or standard padded-base64 `data:text/html`, and
registered `fixture://` documents. A raw `#fragment` is URL metadata, not HTML
payload; an encoded `%23` remains payload data. Successful resource loads
retain the navigation URL, including its fragment, while resource lookup and
data decoding use the URL with the fragment removed.

When the current and target URLs identify the same local resource after
removing the fragment and the URL text changes, navigation is
same-document. It validates the target resource, preserves the current DOM,
origin, layout, control state, and root scroll offset, then commits one new
revision and one bounded history entry. This slice does not perform automatic
anchor scrolling or synthesize fragment targets; that behavior requires a
later layout/input contract.

`NativeEngine::go_back` and `NativeEngine::go_forward` traverse the bounded
history without creating new entries. A target is loaded and, for a different
resource, parsed before the current document or history cursor changes. A
same-resource fragment traversal reuses the current document under the same
same-document rules. A traversal at the history boundary returns `None`
without mutation. A successful new navigation after going back truncates the
forward branch through the existing bounded history policy.

The real `BrowserBackendDispatcher::navigate` path exercises fragment
navigation. Back/forward are explicit Rust `NativeEngine` operations in this
slice because the transport-neutral backend contract has no history operation;
no stable dispatcher capability is invented. All commits remain synchronous,
typed, bounded, and default-off; network, redirects, HTTP, credentials,
cookies, same-origin policy, script, and arbitrary URL schemes remain outside
the contract.

## Tradeoffs

- Fragment navigation now gives local fixtures and data documents observable
  URL/history behavior without adding a network stack or a second DOM owner.
- Traversing a different local resource reparses its bounded body instead of
  storing full document snapshots, which keeps history memory bounded but does
  not preserve mutable state from an earlier document entry.
- Fragment traversal preserves scroll rather than guessing an anchor position;
  this is deterministic but intentionally differs from browser default anchor
  scrolling until element geometry and focus behavior are specified.
- Back/forward remain Rust-only until the shared backend protocol has an
  explicit history operation; adding an unadvertised transport operation would
  create a capability and compatibility contract prematurely.

## Path

- `crates/glass-browser/src/browser/native_engine/config.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/history.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/scheduler.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, feature, SDK, and plan docs

## Verification

- raw-fragment resource handling for `about:blank`, data, and fixture URLs;
- same-document dispatcher navigation preserves document/control/layout state,
  changes URL/revision, and appends bounded history;
- Rust back/forward traversal for same-document and different-document entries,
  boundary no-ops, and forward-branch truncation;
- failed navigation/traversal preserves document, URL, revision, and history;
- native integration and unit suites;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- native-feature doctests and documentation coverage, depth, release-truth,
  version-sync, and feature-parity validators; and
- `cargo fmt --all -- --check`, `git diff --check`, and one focused
  Conventional Commit before the next slice.

## Completion evidence

Implemented in the existing local resource loader, scheduler, engine history,
and real backend dispatcher navigation path. Raw fragments are retained in
successful navigation URLs but removed from resource lookup and data decoding.
Same-resource fragment navigation preserves the document, control state,
layout, and scroll offset while advancing revision/history. Explicit Rust
back/forward traversal reuses same-document entries, reparses different local
resources before activation, preserves failure atomicity, and truncates the
forward branch after a new navigation.

Verified locally on 2026-08-31:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- focused fragment navigation/history test: 1 passed;
- native integration suite: 49 passed;
- native unit suite: 36 passed;
- strict Clippy for default/no-default and `native-engine` feature targets
  passed with warnings denied;
- locked all-target/all-feature `glass-browser` matrix: 819 library tests ran,
  with 818 passed and 1 ignored, and all integration and example targets
  green;
- native-feature Rust doctests: 4 passed; and
- documentation coverage, depth, release-truth, version-sync, and feature
  parity validators passed: 451 Markdown documents, 83 current documents,
  345 full-product MCP tools, 17 examples, 22 public modules, 19 substantive
  contracts, 536 semantic-audit hits, 57 previous-version hits, and 0
  current-claim failures.
