---
id: native-engine-browser-146
scope: glass-browser/native-engine/popup-target-ownership
status: completed
depends-on: [native-engine-browser-145]
---

# Native engine browser slice 146: popup target ownership

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Make the first ordinary new-window navigation a native target-owner behavior.
An allowed activation of an anchor with `target="_blank"` must keep the
opener committed, create one initialized non-selected page target, and expose
the same topology through the runtime, CLI, MCP, and popup-click result.

## Contract

- an actionable native anchor whose target is the reserved `_blank` name
  queues a bounded popup URL instead of navigating its opener;
- download attributes retain precedence over popup creation and continue to
  use the parent-owned download queue;
- local documents, external content-process documents, and page-script
  `element.click()` activation use the same popup ownership rule;
- the native backend materializes queued popup URLs as initialized
  `native-target-N` page owners, records the active opener ID, leaves the
  opener selected, and publishes no half-created target;
- popup target creation inherits the existing native configuration and keeps
  the popup's document, history, content worker, request ledger, prompts,
  downloads, storage reader, and frame tree independent from its opener;
- `BrowserRuntimeSession::native_click_expect_popup`, the native CLI command,
  and native MCP `clickExpectPopup` return causal popup evidence and preserve
  revision guards;
- native `popupOpened` verification observes the live target registry without
  CDP or a hidden browser process;
- popup queue and target-registry limits fail closed, and materialization
  rolls back targets already created in a multi-popup batch when a later
  target cannot initialize;
- named browsing contexts, `window.open` feature strings, popup geometry,
  opener scripting, and user-agent popup blocking remain explicit later
  context/permission work; `_blank` support is not a claim of all popup APIs.

## Tradeoffs

The engine records a popup intent at the page-owner boundary and the backend
then creates a parked target. This keeps the DOM/content worker independent
from target-registry locks and preserves the existing two-crate architecture,
but a target-limit or initialization failure after an accepted page action is
reported as an operation error. The queue is drained and any targets created
in that batch are closed so no stale topology is published.

The public popup result reuses the existing Glass evidence shape. Native
clicks have no physical mouse-release or CDP event sequence, so those fields
carry the native owner witness (`true`, zero transport wait/sequence values,
and an initialized `complete` target) rather than fabricated transport data.

## Implementation surface

- `browser/native_engine/dom.rs`: bounded `_blank` target inspection;
- `browser/native_engine/engine.rs`: bounded popup intent queue and local,
  script, and process-backed link routing;
- `browser/native_backend.rs`: popup materialization, rollback, and generic
  action/script target ownership;
- `browser/runtime.rs`: revision-safe native popup result and topology
  verification;
- `cli/runner.rs`, `mcp/server.rs`: native popup command routing;
- `tests/native_engine.rs`: generic click, explicit popup evidence, script
  activation, opener retention, and independent target routing;
- synchronized architecture, analysis, plan, and package README records.

## Verification

```text
cargo fmt --all -- --check
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine popup -- --nocapture
```

The focused popup integration target passes with 3 tests and 0 failures. The
full native integration/library, strict affected-package lint, workspace
gates, documentation validators, and remote CI remain required before native
production promotion and issue #40 closure.
