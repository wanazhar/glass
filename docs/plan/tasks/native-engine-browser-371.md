# Native Worker and Service Worker response dispatch (371)

```yaml
id: native-engine-browser-371
scope: native-engine/worker-response-transport
status: done
depends-on:
  - native-engine-browser-370
```

## Objective

Remove generated JavaScript source from the remaining Worker and Service
Worker response continuations. Host responses must reach installed resolver
functions as structured values, so a valid response body cannot consume the
16 KiB authored-script source budget or be converted into executable source.

## Delivered behavior

- `NativeWorkerDispatch` now carries Worker Fetch, Service Worker nested Fetch,
  CacheStorage, and `clients.openWindow()` settlements alongside the existing
  message and MessagePort events.
- Dedicated, shared, classic, and module Worker Fetch responses are delivered
  through the existing worker-turn bootstrap and the installed
  `__glassResolveWorkerFetch` function.
- Service Worker nested Fetch, CacheStorage, and openWindow responses use the
  same direct worker-realm boundary and their installed resolver names; the
  worker host evaluates only the bounded `undefined;` continuation afterward.
- Response envelopes are serialized once at the host boundary and parsed as
  QuickJS JSON values under the existing 16 MiB native command envelope. The
  resource-loader response, header, body, cache, IPC, and Service Worker
  limits remain authoritative.
- The process-backed HTTP witnesses cover a 20,000-byte Worker Fetch response,
  a 20,000-byte Service Worker nested Fetch response, and a 20,000-byte
  CacheStorage match response. The existing Service Worker openWindow
  continuation witness remains green.

## Contract and tradeoffs

The boundary is structured, but response bodies still use the existing
JSON/base64 projection and finite host-memory/IPC budgets. This removes the
incorrect authored-source coupling; it does not make response bodies
unbounded or change stream ownership. Fetch stream event transport, global
task-source arbitration, and broader browser conformance remain separate
issue #40 gates. Resolver callbacks are still delivered on an operation-bound
worker turn, preserving the current serialized mutation and command queues.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-371.md`

## Verification

The following checks passed locally against this slice:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_worker_fetch --locked -- --nocapture` — 3 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_registers_service_worker_and_intercepts_fetch_and_navigation --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_service_worker_cache_matching_options_filter_entries --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_runtime_service_worker_clients_open_window_materializes_window_client --locked -- --nocapture` — 1 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local checkpoint.
