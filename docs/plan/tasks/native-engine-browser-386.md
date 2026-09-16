# Native cross-target MessagePort routing (386)

```yaml
id: native-engine-browser-386
scope: native-engine/browser-message-port-routing
status: done
depends-on:
  - native-engine-browser-385
```

## Objective

Complete the browser-owned half of WindowProxy `MessagePort` transfer. A port
received by a page in another native target must be able to send messages back
to its original peer, including a new transfer list, across both local and
content-process-backed documents.

## Contract

- Page-owned MessagePort commands that do not terminate in a local Worker are
  carried as bounded typed records through the page and content-process
  boundaries.
- The browser backend owns a bridge-key route to the source target/frame and
  dispatches the command to the receiving page's native MessagePort registry.
- A receiving port may transfer another MessagePort back; the returned bridge
  is registered to the command source before later commands are delivered.
- Target-origin filtering and existing structured-clone, bridge-key,
  descriptor, payload, effect, and topology limits remain enforced.
- Route insertion is atomic and globally bounded. Routes owned by a navigated,
  closed, or removed target/frame are cleared or pruned, so a fresh JavaScript
  realm cannot collide with an old port ID.
- The content-process response remains JSON-safe and contains no host pointer
  or live JavaScript object; source context/frame metadata is filled only by
  the browser owner.

## Delivered behavior

- Added a bounded `NativePageMessagePortCommand` record and carried it through
  local engine effects, content-worker responses, and the browser effect
  scheduler.
- Added a round-robin browser task-source branch for page MessagePort commands
  so nested popup, ordinary message, navigation, close, Worker, and port
  effects continue to drain without starving one another.
- Routed commands to active and parked top-level targets and nested frames;
  returned transfer descriptors are registered against the command source.
- Added a hard route-table bound, duplicate-safe atomic insertion, owner
  validation, stale-route pruning, and navigation/close cleanup.
- Added popup tests for one-way and bidirectional transfer-list routing,
  navigation key reuse, and an HTTP/content-process transfer-list round trip.

## Tradeoffs and explicit follow-up

The browser route map adds a small synchronized lookup and bounded bookkeeping
cost to each transferred port. Keeping the map in Rust preserves the target
and frame ownership boundary, but it does not yet provide a complete browser
task scheduler or full Web Platform event-loop conformance. ArrayBuffer
detachment, complete transferability for every cloneable platform object,
browser-wide task-source ordering, and the remaining Core Web Profile and
production certification gates remain issue #40 work.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-386.md`

## Verification

Validation completed locally against this checkpoint:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_window_proxy_message_port --locked` — 2 passed
- `cargo test --quiet -p glass-browser --test native_engine native_window_proxy_message_port_routes_reset_with_navigation --locked` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_http_post_message_crosses_content_worker_and_replies --locked` — 1 passed
- `cargo fmt --all -- --check`
- documentation depth, coverage, TUI-shortcut, and release-documentation validators
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
