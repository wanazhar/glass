# Glass native engine browser slice 252: persistent WebSocket transport

Status: completed locally.

## Objective

Give page JavaScript a real, persistent WebSocket transport owned by the
native content process. A page must be able to construct a `WebSocket`, pass
through the native URL/security policy, exchange text and binary frames, and
observe lifecycle events without a CDP or alternate-browser fallback.

## Contract

- `WebSocket` construction resolves relative URLs against the committed
  document, accepts only `ws:`/`wss:` targets without credentials, and emits a
  typed open command from the page realm.
- The content worker owns the handshake and connection lifetime. It sends the
  document `Origin`, eligible session cookie state, and validated subprotocols
  through the existing connect/mixed-content policy.
- Outbound text, Blob, ArrayBuffer, and typed-array data use bounded typed
  commands and bounded Tokio channels. Inbound text and binary frames are
  bounded before they cross into JavaScript.
- Open, message, error, and close events are delivered through the same
  serialized JavaScript/event/mutation owner as timers and fetch continuations.
  A later ordinary evaluation drains already-queued socket events instead of
  leaving them stranded behind a completed script request.
- Close codes and reasons are validated at both the JavaScript and Rust
  boundaries. Server Ping frames receive a client Pong response, and transport
  failures surface as error plus abnormal-close events.
- The page-load path can create a socket from an inline script; event handlers
  can mutate the document and issue follow-up socket commands before the
  resulting snapshot is published.

## Implementation

`NativeScriptCommand` now carries WebSocket open/send/close operations, while
the persistent QuickJS bootstrap owns the WebSocket object map, lifecycle
state, listener dispatch, binary conversion, protocol validation, and bounded
payload encoding. `NativeResourceLoader::websocket_target` centralizes
credential, scheme, mixed-content, connect-src/default-src, and cookie policy.

The content worker creates one bounded Tokio task per socket. The task performs
the handshake with `tokio-tungstenite`, handles outbound commands and inbound
frames, answers Ping frames, and queues typed events. The worker event-loop
owner dispatches those events through the persistent realm and applies all
resulting DOM/history/fetch work transactionally before returning the updated
document wire.

## Tradeoffs and follow-up

The transport deliberately uses bounded per-socket channels and an event-turn
cap, so a hostile peer cannot grow unbounded worker memory or monopolize one
content request. Session cookies are read for the handshake but response
cookie persistence, full WebSocket Web IDL descriptors, background scheduling,
workers, Service Workers, EventSource, and the remaining browser-complete
resource and lifecycle surfaces are separate issue #40 work. This checkpoint
does not claim native/CDP parity or production promotion by itself.

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_drives_websocket_text_binary_and_close_events --locked -- --nocapture` (1 passed)
- `git diff --check`

The evidence is local-only. Remote CI, push, release, registry publication,
and final native-engine parity claims remain unclaimed for this checkpoint.
