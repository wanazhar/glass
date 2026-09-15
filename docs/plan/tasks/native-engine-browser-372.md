# Native network event dispatch (372)

```yaml
id: native-engine-browser-372
scope: native-engine/network-event-transport
status: done
depends-on:
  - native-engine-browser-371
```

## Objective

Remove generated JavaScript source from page WebSocket, EventSource, and
Fetch-stream events and from dedicated/shared Worker WebSocket and EventSource
events. Host payloads must reach the already-installed realm dispatchers as
structured data without consuming the authored-script source budget.

## Delivered behavior

- `NativePageDispatch` carries page WebSocket, EventSource, and Fetch-stream
  events alongside Fetch response settlements.
- `NativeWorkerDispatch` carries dedicated/shared Worker WebSocket and
  EventSource events alongside Worker and Service Worker response settlements.
- Each event payload is serialized once at the bounded host boundary, parsed as
  a QuickJS value, and passed to its installed dispatcher. The host evaluates
  only the existing `undefined;` continuation afterward.
- Page callbacks use `__glassDispatchWebSocketEvent`,
  `__glassDispatchEventSourceEvent`, and
  `__glassDispatchFetchStreamEvent`; worker callbacks use the corresponding
  installed Worker dispatchers. Existing task-source queues, ordering, and
  transport-specific limits are unchanged.
- Process-backed HTTP(S) witnesses cover 20,000-byte page WebSocket and
  EventSource payloads, a 20,000-byte Fetch-stream body, and 20,000-byte
  dedicated/shared Worker WebSocket and EventSource payloads.

## Contract and tradeoffs

The structured boundary removes payload-size coupling to generated source, but
does not make transport data unbounded. JSON/base64 conversion, the 16 MiB
native command envelope, resource-loader/IPC limits, and per-transport limits
remain authoritative. Fetch stream ownership and socket framing remain
incremental, so one server write is not required to equal one JavaScript stream
read. This slice closes the remaining network-event paths covered by the
current transport protocol; browser-wide task-source arbitration, autonomous
background scheduling, and the remaining Core Web Profile gates remain open.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-372.md`

## Verification

The following checks passed locally against this slice:

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine websocket --locked -- --nocapture` — 4 passed
- `cargo test --quiet -p glass-browser --test native_engine event_source --locked -- --nocapture` — 3 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_dispatches_large_fetch_stream_chunk_as_data --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_worker_dispatches_large_ --locked -- --nocapture` — 2 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
