# Native Service Worker client messages (345)

Status: implemented locally in the current 0.3.14 checkout.

This slice completes the active-client messaging path adjacent to
`clients.matchAll()`:

- a client returned by native `clients.matchAll()` implements
  `Client.postMessage()` with the existing structured JSON clone and bounded
  MessagePort transfer list;
- the isolated worker command validator checks worker ownership, opaque client
  identity, payload size, and transfer descriptors before the native owner
  settles the event;
- the content owner queues messages only for the currently dispatched page
  client, delivers them to the page's ServiceWorker container as a
  `MessageEvent`, and reuses the existing bridge for transferred ports; and
- queued client events are injected at the next bounded page evaluation (or
  fetch continuation), while navigation clears stale client events and routes.

## Tradeoffs

- The current content-process topology has one top-level page client, so a
  message addressed to another tab/frame is validated and dropped rather than
  being delivered to an incorrect page. A browser-wide client ledger and
  exact multi-context task-source scheduling remain issue #40 gates.
- Messages emitted during a Service Worker Fetch are delivered through the
  fetch continuation before the page observes its resolved result. Messages
  produced during other worker turns wait for the next native page turn;
  this preserves bounded ownership without inventing a parallel event loop.
- Transfer descriptors use the existing owner-routed MessagePort bridge and
  remain bounded by the shared message limits. Rich structured-clone types,
  `clients.openWindow()`, and complete lifecycle/version ordering remain
  separate profile gates.

## Local evidence

- `cargo fmt --all -- --check`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo check --quiet -p glass-browser --bins --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine native_content_process_registers_service_worker_and_intercepts_fetch_and_navigation --locked` — 1 passed
- `git diff --check`

The HTTP(S) integration witness registers and activates a worker, enumerates
the current client, sends a cloned message with one transferred MessagePort,
and verifies the page receives the message, port count, source worker, and
fetch result. All evidence is local; this checkout has not been pushed and has
no remote CI result.
