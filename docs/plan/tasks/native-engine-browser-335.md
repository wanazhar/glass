# Native message-channel surfaces (335)

Status: implemented locally in the current 0.3.14 checkout.

This slice adds the message-channel primitives that page and dedicated-worker
scripts use without introducing a second transport or scheduler:

- `MessageChannel` creates two entangled `MessagePort` objects with bounded
  queued delivery, `start()`, `close()`, `postMessage()`, `onmessage`, and
  listener-based dispatch;
- `MessagePort` and `BroadcastChannel` expose stable constructor identity and
  the native `EventTarget` relationship in both page and worker realms;
- `BroadcastChannel` delivers asynchronously to other open channels with the
  same name in the owning realm and never echoes to the sender;
- message payloads are cloned before delivery, bounded by the existing native
  message limit, and invalid/cyclic or non-empty transferable lists fail with
  typed errors; and
- page and worker delivery use their existing QuickJS microtask queues, so
  message callbacks remain ordered with the surrounding script turn.

The slice deliberately keeps channel ownership inside the existing JavaScript
realm. Cross-context `MessagePort` transfer, transferables, shared workers,
service workers, and browser-wide task-source scheduling still require the
issue #40 promotion work that will connect those realms through the native
owner rather than pretending same-realm delivery is full browser parity.

## Local evidence

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_local_message_channels_deliver_events_in_page_and_worker_realms --locked -- --nocapture` — 1 passed

All evidence is local; this checkout has not been pushed and has no remote CI
result.
