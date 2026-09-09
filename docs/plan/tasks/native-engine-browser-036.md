---
id: native-engine-browser-036
scope: glass-browser/native-engine/parser-script-timing
status: done
depends-on: [native-engine-browser-035]
---

# BE-03m/BE-04r: bounded parser-time script ordering

## Objective

Make ordinary page-script initialization observe the important parser-time
ordering boundary instead of executing every discovered script as one flat
document-order list.

## Contract

- Classic scripts without `async` or an applicable `defer` attribute retain
  parser-blocking order.
- External classic `async` scripts are admitted as a separate bounded
  post-discovery turn before deferred scripts. Their source order is retained
  inside that deterministic async bucket.
- External classic `defer` scripts and module roots (which are deferred by
  default) execute in document order after parser-blocking and async buckets.
  Static module dependencies retain the root's scheduling bucket.
- Inline classic `async`/`defer` attributes retain the HTML boundary that
  those attributes do not create asynchronous external-script loading; inline
  modules remain deferred.
- Local and child-owned script discovery use the same timing metadata and
  ordering helper. Existing CSP, MIME, redirect, referrer/cookie, source,
  graph, job, and document limits remain in force.

## Deliberate boundary and tradeoffs

- This is deterministic bounded scheduling, not a full incremental HTML
  parser or wall-clock async race model. External resources are still loaded
  through the existing bounded loader before publication; the engine does not
  claim completion-order races, `document.write`, dynamic script insertion,
  `DOMContentLoaded`/`load` timing, preload/modulepreload, or parser recovery.
- The async bucket runs before the defer bucket so pages get a stable
  initialization order without pretending network timing is deterministic.
  Real browser task-source ordering and script event callbacks remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_scripts_honor_bounded_parser_timing_order -- --nocapture` — 1/1 passed.
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_honors_bounded_async_and_defer_script_order -- --nocapture` — 1/1 passed.
- `git diff --check`

Remote CI, push, release, tag, registry publication, browser-parity, and
issue-closure claims remain pending the wider browser-complete gates.
