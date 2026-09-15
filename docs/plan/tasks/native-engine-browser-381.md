# Native SharedWorker connection structured transport (381)

```yaml
id: native-engine-browser-381
scope: native-engine/shared-worker-connect-transport
status: done
depends-on:
  - native-engine-browser-380
```

## Objective

Remove the remaining generated-source boundary from SharedWorker connection
delivery. A transferred MessagePort descriptor must enter the worker realm as
structured data while preserving named-worker reuse, connect ordering, port
ownership, and module/classic behavior.

## Delivered behavior

- `NativeWorkerDispatch::SharedWorkerConnect` carries validated transfer
  descriptors through the existing worker event-dispatch channel.
- The worker owner parses the connection envelope once and calls the installed
  `__glassDispatchSharedWorkerConnect` function during a static `undefined;`
  continuation.
- Initial SharedWorker evaluation and subsequent named-worker connections use
  the same typed path for local fixture and HTTP(S) content-process owners.
- The previous connection-specific JSON-to-source formatter is removed.
- Existing bridge-key, port-count, worker identity, route registration, and
  cleanup validation remains authoritative.

## Contract and tradeoffs

The worker owner performs one bounded structured payload parse for each connect
turn. This adds a small host-boundary cost but prevents transfer descriptors
from sharing an executable source string with worker code and keeps the
connection transport consistent with message, fetch, and network dispatch.
The connect event itself remains an installed JavaScript DOM-style worker
callback because that is where the worker event listener and port projection
live; Rust retains ownership of descriptor validation and route identity. Full
worker task-source scheduling, richer transferable parity, Core Web Profile
conformance, cross-platform certification, and production release evidence
remain open issue #40 gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-381.md`

## Verification

The following checks passed locally against this slice:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_local_shared_worker_reuses_named_runtime_and_ports --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_shared_worker_reuses_named_runtime_and_ports --locked -- --nocapture` — 1 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
