# Native service-worker MessagePort transfer (338)

Status: implemented locally in the current 0.3.14 checkout.

This slice extends the native cross-realm transfer bridge from dedicated
workers to active service workers:

- `ServiceWorker.postMessage(message, transfer)` accepts the same bounded
  `MessagePort` transfer descriptors as page and dedicated-worker realms;
- the service-worker runtime receives transferred endpoints through
  `MessageEvent.ports`, can call `start()` and install an `onmessage` handler,
  and can send a response through the endpoint;
- page-owned bridge routes are registered and validated by the
  `NativeServiceWorkerRegistry`, then dispatched by worker identity rather
  than by a JavaScript object or an untrusted page-provided worker reference;
- worker-produced messages are queued for the next bounded page turn, with
  transferred return ports preserved in the page event; and
- routes and queued messages are invalidated on page navigation/reload and
  removed when a registration is replaced or unregistered. Transfer lists,
  bridge keys, command counts, and payloads remain bounded.

The service-worker evaluator now explicitly awaits fetch dispatch and
normalizes the worker result envelope before decoding `respondWith()` output.
This keeps promise settlement and host command collection consistent with the
page evaluator and prevents an unhandled Promise wrapper from silently
falling through to the network.

## Tradeoffs

- Service-worker channels use the existing owner queue and bridge-key registry,
  so the two native execution paths share one auditable routing contract. They
  do not add a third JavaScript runtime or a hidden CDP dependency.
- MessagePort delivery is bounded and drained at a page turn. It preserves
  deterministic ownership and resource limits, while complete browser task
  source interleaving and richer transferable types remain profile work.
- Registration state remains process-owned for this slice. Durable worker
  script/cache persistence and SharedWorker ownership are separate promotion
  gates.

## Local evidence

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine service_worker --locked -- --nocapture` — 2 passed

The HTTP(S) integration witness covers registration and activation, navigation
and Fetch interception, unregister fallback, page-to-service-worker transfer,
source detachment, `MessageEvent.ports`, and a service-worker reply delivered
back to the page. All evidence is local; this checkout has not been pushed and
has no remote CI result.
