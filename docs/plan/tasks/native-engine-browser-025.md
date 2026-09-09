---
id: native-engine-browser-025
scope: glass-browser/native-engine/navigation-url-history
status: done
depends-on: [native-engine-browser-024]
---

# BE-02g/BE-07a: relative URL and same-document script navigation

## Objective

Make script link activation behave correctly for ordinary external-page link
shapes: resolve relative HTTP(S) references against the current document and
keep fragment-only navigation in the existing document/history owner.

## Contract

- Link resolution accepts absolute URLs, fixture-relative references under the
  existing fixture-host policy, and relative/root-relative HTTP(S) references
  resolved with the current URL as base. Resolved values pass the existing URL
  text/security validation before loading.
- Local script fragment links reuse the current document and realm, update the
  URL/history/scroll owner, and do not reparse the page.
- External child script fragment links use the parent history/revision commit
  while retaining the child process and page realm; no redundant HTTP load is
  issued for same-document navigation.
- External non-fragment script links remain child-owned: the child transfers a
  validated link request and the parent performs the next asynchronous child
  load.

## Deliberate boundary and tradeoffs

- Relative resolution does not yet implement every URL/base-element edge case,
  document base URL mutation, service-worker interception, download targets,
  popup/opener contexts, or cross-origin policy beyond the existing navigation
  loader.
- Script click and navigation remain separate revisions. Form submission,
  `beforeunload`/unload/page lifecycle, task ordering, history state payloads,
  and navigation cancellation remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- focused local same-document fragment test
- focused child relative-navigation test
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture` — 292/292 passed.
- `git diff --check`

The next gate is navigation lifecycle/default-action ordering and target
contexts, followed by timers, modules, page-script loading, and Fetch/XHR.
