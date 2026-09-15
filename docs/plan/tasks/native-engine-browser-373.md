# Native Service Worker transport dispatch (373)

```yaml
id: native-engine-browser-373
scope: native-engine/service-worker-transport
status: done
depends-on:
  - native-engine-browser-372
```

## Objective

Remove generated JavaScript source from Service Worker Fetch request,
lifecycle, and page registration-result transport. Host-owned values must
enter the already-installed worker or page callbacks as structured data while
preserving the existing `respondWith()`, `waitUntil()`, and Promise settlement
semantics.

## Delivered behavior

- Service Worker Fetch request envelopes are parsed once at the QuickJS host
  boundary and passed directly to `__glassDispatchServiceWorkerFetch`.
- Service Worker lifecycle event types are passed as structured values to
  `__glassDispatchServiceWorkerLifecycle`; the returned Promise is awaited by a
  bounded static continuation, preserving install/activate `waitUntil()` work.
- The page Service Worker registration/unregistration/update resolver now uses
  the same structured page dispatch boundary as page Fetch and network events.
- A single serialized host-turn Promise slot is used for the two awaitable
  Service Worker dispatches; fulfillment and rejection clear it before the
  existing worker settlement loop consumes commands and results.
- A process-backed HTTP(S) witness sends a 20,000-byte POST body through a
  controlled Service Worker, consumes it with `event.request.text()`, and
  returns its length through `respondWith()`.

## Contract and tradeoffs

The request and lifecycle values no longer consume the authored JavaScript
source budget, but remain finite structured payloads. JSON/base64 conversion,
the 16 MiB native command envelope, request/body/IPC limits, and existing
Service Worker event-loop bounds remain authoritative. The direct callback
invocation still runs on the serialized worker owner; it does not add a
resident background loop or claim browser-wide task-source arbitration.
Registration payloads and response bodies remain bounded, and full Core Web
Profile conformance, recovery/cancellation, and cross-platform certification
remain open issue #40 gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-373.md`

## Verification

The following checks passed locally against this slice:

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_service_worker_dispatches_large_fetch_request_as_data --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine service_worker --locked -- --nocapture` — 17 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
