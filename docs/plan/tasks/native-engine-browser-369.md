# Native structured cross-realm event dispatch (369)

```yaml
id: native-engine-browser-369
scope: native-engine/cross-realm-event-transport
status: done
depends-on:
  - native-engine-browser-368
```

## Objective

Remove the native browser engine's accidental coupling between event-payload
size and JavaScript source size. A valid Worker, MessagePort, or Service
Worker client message must be able to use the existing bounded message
contract without being rejected merely because the generated dispatch source
would exceed the 16 KiB script-source limit.

## Delivered behavior

- Page-facing Worker, MessagePort, and Service Worker client events are
  admitted as a structured `NativePageEventBatch` and dispatched through the
  already-installed QuickJS functions before the user script runs.
- Page-to-dedicated/shared-worker Worker and MessagePort delivery uses direct
  QuickJS value injection instead of concatenating JSON into generated source.
- Service Worker messages and Service Worker MessagePort delivery use the same
  direct worker-realm dispatch path.
- Local and content-process page turns share the same batch shape, queue
  ordering, per-queue admission cap, transfer validation, and 256 KiB encoded
  message limit.
- Fetch-continuation turns retain their bounded response-source contract while
  queued Service Worker client events travel through the structured page-event
  path.
- The content-process IPC envelope carries the additive page-event batch and
  drains previously queued worker, MessagePort, and Service Worker client
  events into it before evaluation.
- Local and HTTP(S) integration witnesses prove a 20,000-byte Worker message
  can round-trip to the page without consuming the 16 KiB script-source
  budget.

## Contract and tradeoffs

Payloads are serialized for bounded validation and entered into QuickJS with
`Ctx::json_parse`; they are not interpolated into executable source. This
preserves the existing JSON-backed structured-clone subset and transfer-port
validation while separating the payload budget from the user-script budget.
Each event queue remains capped at `MAX_NATIVE_WORKER_MESSAGES`, and every
message remains capped at `MAX_NATIVE_POST_MESSAGE_BYTES`. Delivery preserves
the established Service Worker client, MessagePort, then Worker order within a
host turn.

This slice does not claim a resident background event loop, one global order
across every page/worker/network/rendering/browser-context task source, or
complete binary structured-clone parity. Fetch response continuation still
uses its existing bounded source-transfer fallback and is a separate response
transport gate. Those limits remain explicit issue #40 work rather than being
hidden by the new event path.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-369.md`

## Verification

The following checks passed locally against the committed slice:

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine large_payloads --locked -- --nocapture` — 2 passed
- `cargo test --quiet -p glass-browser --test native_engine native_local_message_channels_deliver_events_in_page_and_worker_realms --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_transfers_message_ports_between_page_and_worker_realms --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_service_worker_transfers_message_port_round_trip --locked -- --nocapture` — 1 passed
- Service Worker-filtered native integration binary — 16 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
