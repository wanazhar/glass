# Native Service Worker cross-target `WindowClient.postMessage()` (360)

```yaml
id: native-engine-browser-360
scope: native-engine/service-worker-client-message-routing
status: done
depends-on:
  - native-engine-browser-359
```

## Objective

Complete the browser-owned delivery half of Service Worker client messaging.
`WindowClient.postMessage()` must be able to address the exact target returned
by `clients.openWindow()` or enumerated by `clients.matchAll()`, including a
parked top-level target, without selecting that target or falling back to CDP.

## Delivered behavior

- The content process keeps same-client Service Worker messages on the existing
  local delivery path while exporting messages addressed to another client as
  bounded typed IPC records.
- Client-message records carry the worker id, opaque browser client id,
  structured-cloned data, and validated MessagePort transfer descriptors.
- The native engine retains cross-target records as browser-owned effects and
  drains them through the same bounded effect handoff used by popups,
  navigation, close, and `clients.openWindow()`.
- The browser backend resolves the opaque client id against the selected target,
  parked target, selected frame, or parked frame and dispatches the page
  `ServiceWorkerContainer` `MessageEvent` in that exact native frame owner.
- Nested popup, message, close, navigation, `openWindow()`, and client-message
  effects are re-enqueued, and a sixth round-robin source prevents a persistent
  client-message stream from starving the other browser-owned sources.
- The HTTP integration witness now opens `/opened`, calls
  `WindowClient.postMessage()` from the worker continuation, selects the parked
  target, and verifies that the page receives the structured payload.

## Contract and tradeoffs

The opaque client id is the routing key; the target is never inferred from URL,
window name, current selection, or array position. This preserves exact target
identity when several same-origin tabs or frames have similar URLs. The route
is serialized at the browser topology owner, so delivery is deterministic but
not physically concurrent across target owners.

Message data and transfer descriptors reuse the existing bounded structured
clone and MessagePort bridge. Rich transferables, full MessageEvent/Web IDL
descriptor parity, browser-wide registration arbitration, fetch-event
suspension, durable live-client leases, and complete task-source scheduling
remain separate issue #40 gates. A stale or unknown client id fails closed as a
selection error rather than delivering to the active page.

## Touched paths

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/mod.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-360.md`

## Verification

The following checks passed locally against the current checkout:

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_DEV_DEBUG=0 cargo check --quiet -p glass-browser --lib --bins --test native_engine --locked`
- `CARGO_PROFILE_DEV_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_runtime_service_worker_clients_open_window_materializes_window_client --locked` — 1 passed
- `CARGO_PROFILE_DEV_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_runtime_service_worker_clients --locked` — 2 passed
- `CARGO_PROFILE_DEV_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_registers_service_worker_and_intercepts_fetch_and_navigation --locked` — 1 passed
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation.json` — 1,010 Markdown files; 83 current documents; 62 previous-version hits; 1,165 semantic audit hits; 0 current-claim failures
- `python3 scripts/check-documentation-depth.py` — 93 current guides and 19 substantive contracts
- `python3 scripts/check-tui-shortcuts.py` — 15 implementation help keys and 63 documentation markers
- `python3 scripts/check-documentation-coverage.py` — 1,010 Markdown files; 346 full-product MCP tools; 17 examples; 22 public modules
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
