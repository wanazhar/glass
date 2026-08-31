---
id: native-engine-038
scope: glass-browser/native-engine/local-link-activation
status: done
depends-on: [native-engine-037]
---

# Native bounded local link activation

## Objective

Wire semantic `<a href>` clicks through the existing native action path and
bounded local navigation owner, so the native engine has one observable local
default action without inventing a new backend operation or a script/event
loop.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_backend.rs`

## Contract

The existing `NativeAction::Click` and transport-neutral `SemanticAction::Click`
operations remain the only entry point. When the resolved actionable element is
an anchor with `href`, a non-empty supported local destination is treated as a
bounded navigation default action:

- fragment-only `href="#fragment"` targets the current local resource with the
  new raw fragment, preserving the current resource body and same-document
  state rules;
- absolute `about:blank`, `data:text/html`, and registered `fixture://` hrefs
  are passed to the existing loader, including any raw fragment; and
- an empty href has no navigation default action and retains the existing
  bounded click behavior.

Link destinations are validated and, for a different resource, parsed before
the current document is committed or the history cursor changes. A failed or
unsupported link destination leaves the current document, URL, revision,
history, and control state unchanged. A successful same-resource link
preserves the current document, control state, layout, and root scroll offset
while committing one revision/history entry. A successful different-resource
link commits one parsed document, resets root scroll to zero, and appends one
history entry. The action result reports the resulting revision.

Relative paths, remote schemes, filesystem URLs, malformed URLs, and all other
resource forms remain rejected with typed bounded errors. The link target is
never echoed in an error or diagnostic. This slice does not add event
propagation, keyboard activation, target browsing contexts, download behavior,
redirects, network fetching, scripting, or transport-level history methods.

## Tradeoffs

- Local anchor activation exercises the real action-to-navigation chain and
  makes the existing fragment/history behavior reachable through user-facing
  semantics without adding a second navigation owner.
- Limiting hrefs to fragment-only or absolute local resources avoids silently
  inventing URL-base, filesystem, network, or origin policy before the resource
  security workstream defines those contracts.
- Cross-document activation preserves the existing bounded parse-before-commit
  rule but does not retain mutable state from the old document or implement
  event/default-action ordering beyond this one synchronous path.
- Empty hrefs remain clicks without a reload/navigation entry; this keeps the
  action revision contract deterministic until same-URL reload semantics are
  specified.

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized native-engine architecture, analysis, feature, SDK, and plan docs

## Verification

- real dispatcher click on a fragment-only link preserves the bounded document
  and scroll state while changing URL/revision/history;
- real dispatcher click on an absolute local link parses and commits the new
  document;
- unsupported/relative link destinations fail before mutation and do not echo
  the destination;
- empty href retains bounded click behavior without navigation;
- native integration and unit suites;
- strict Clippy for default and `native-engine` feature targets;
- full locked `glass-browser` all-target/all-feature test matrix;
- native-feature doctests and documentation coverage, depth, release-truth,
  version-sync, and feature-parity validators; and
- `cargo fmt --all -- --check`, `git diff --check`, and one focused
  Conventional Commit before the next slice.

## Completion evidence

Implemented in the existing native DOM and engine action path. Actual anchors
with non-empty local hrefs now preflight their destination and invoke the
existing same-document or parsed navigation commit. Fragment-only links retain
the current resource and bounded state; absolute local links can commit a new
document; empty hrefs remain click-only; and unsupported or relative hrefs fail
before mutation without raw-target echo.

Verified locally on 2026-08-31:

- `cargo fmt --all -- --check` and `git diff --check` passed;
- focused local-link activation test: 1 passed;
- native integration suite: 50 passed;
- native unit suite: 36 passed;
- strict Clippy for default/no-default and `native-engine` feature targets
  passed with warnings denied;
- locked all-target/all-feature `glass-browser` matrix: 819 library tests ran,
  with 818 passed and 1 ignored, and all integration and example targets
  green;
- native-feature Rust doctests: 4 passed; and
- documentation coverage, depth, release-truth, version-sync, and feature
  parity validators passed: 452 Markdown documents, 83 current documents,
  345 full-product MCP tools, 17 examples, 22 public modules, 19 substantive
  contracts, 536 semantic-audit hits, 57 previous-version hits, and 0
  current-claim failures.
