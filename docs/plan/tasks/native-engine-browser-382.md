# Native Service Worker client projection structured transport (382)

```yaml
id: native-engine-browser-382
scope: native-engine/service-worker-client-projection
status: done
depends-on:
  - native-engine-browser-381
```

## Objective

Remove live Service Worker client projections from generated worker bootstrap
source. Browser-owned client state must enter the Service Worker realm as a
bounded structured value before scripts, lifecycle callbacks, timers, message
events, or Fetch continuations observe it.

## Delivered behavior

- The Service Worker bootstrap initializes an empty client projection and
  installs `__glassSetServiceWorkerClients` as the page-owner update function.
- Every Service Worker evaluation turn sends the current client array through a
  parsed structured payload before dispatching worker code or an event.
- Initial script evaluation, lifecycle/fetch/cache/open-window continuations,
  timers, and Service Worker messages all share the same update boundary.
- Client payload size is bounded by the existing Service Worker client lease
  limit, while the existing client identity, control, visibility, type, and
  frame projections remain unchanged.
- The previous client-array bootstrap serialization and placeholder replacement
  are removed.

## Contract and tradeoffs

The worker owner pays one bounded client-array JSON parse on each Service Worker
turn. In exchange, changing client URLs, identities, or control state no longer
changes executable bootstrap source or consumes the authored worker-script
budget. The installed worker JavaScript still owns the `clients.matchAll()` and
`WindowClient` projection semantics; Rust owns client identity, validation,
lease limits, and update timing. Fetch payloads may still refresh the projection
as part of their existing request envelope. Full worker task-source scheduling,
Core Web Profile conformance, cross-platform certification, and production
release evidence remain open issue #40 gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-382.md`

## Verification

The following checks passed locally against this slice:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test -p glass-browser --lib native_service_worker_client_tests::service_worker_clients_accept_maximum_url_without_bootstrap_source_coupling --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_runtime_service_worker_clients_match_all_across_targets --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_runtime_service_worker_client_leases_cross_sessions_and_close --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_registers_service_worker_and_intercepts_fetch_and_navigation --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_runtime_service_worker_clients_open_window_materializes_window_client --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_service_worker_transfers_message_port_round_trip --locked -- --nocapture` — 1 passed
- `cargo fmt --all -- --check`
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
