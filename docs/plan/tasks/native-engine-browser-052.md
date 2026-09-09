---
id: native-engine-browser-052
scope: glass-browser/native-engine/page-load-fetch
status: done
depends-on: [native-engine-browser-051]
---

# BE-02x/BE-04ah: page-load fetch settlement

## Objective

Carry bounded GET fetch requests emitted by parser/lifecycle page scripts
through document commit, resolving their promise callbacks before the child
publishes the loaded document snapshot.

## Contract

- Inline/classic/module page scripts may enqueue the same bounded GET fetch
  commands as explicit evaluations.
- The child resource loader resolves those requests before the `loaded` IPC
  response; response promise callbacks run in the persistent page realm.
- Typed DOM mutations from those callbacks are included in the first published
  document snapshot, so parent evidence does not observe an intermediate page.
- Fetch callback navigation during initial publication remains an explicit
  denial; later navigation/task integration is separate.

## Deliberate boundary and tradeoffs

Settlement is bounded and serialized with initial document commit. It does not
create an independent network scheduler, parallel fetch queue, or full HTML
fetch task model. Non-GET requests, upload streams, AbortController, service
workers, XHR/WebSocket, and full Fetch Web IDL identity remain open.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `native_content_process_resolves_page_script_fetch_before_publish`
- complete fetch-related subset: 4 passed
- `cargo fmt --all -- --check`
- `git diff --check`

Remote CI, source push, release, tag, registry publication, browser-parity,
and issue-closure claims remain pending the wider browser-complete gates.
