# Glass native engine browser slice 292: worker WebSocket bridge

Status: completed locally.

## Objective

Give dedicated workers the same persistent WebSocket transport class already
available to the page realm, with explicit ownership through the isolated
worker/content-process boundary.

## Contract

- A dedicated worker can construct a bounded WebSocket with a `ws:` or `wss:`
  URL, validate protocols, observe `CONNECTING`/`OPEN`/`CLOSING`/`CLOSED`, and
  send text, Blob, ArrayBuffer, and typed-array payloads.
- Worker-owned open, send, and close commands carry the worker identifier and
  cannot be applied to a different worker or a page-owned socket.
- The HTTP(S) content process resolves the target through the shared resource
  loader and worker URL/origin policy, then reuses the existing transport task
  for handshake, ping/pong, text/binary frames, transport errors, and clean
  close events.
- Worker callbacks run inside the isolated QuickJS realm. Their page messages,
  network commands, and lifecycle effects are returned through the existing
  bounded page-turn queue.
- A local fixture-owned engine reports that process-backed WebSocket transport
  is required instead of silently discarding a worker command.

## Implementation

`NativeScriptCommand` now carries an optional worker owner for WebSocket
operations. `NativeWorkerRegistry` retains worker URL context and queues
owner-tagged commands; it can dispatch serialized transport events back into
the worker realm and collect callback output without exposing the page realm.
The worker bootstrap provides a bounded WebSocket constructor, event listener
surface, binary conversion, protocol/close validation, and persistent socket
identity across host turns. The content process maintains worker connections
separately from page connections, pumps one worker event per page turn, and
routes resulting sends/closes through the existing WebSocket task.

## Tradeoffs and follow-up

Reusing the page transport keeps handshake, framing, ping/pong, close, and
resource-policy behavior in one owner, while the `(worker_id, socket_id)` key
prevents cross-realm confusion. Delivery remains page-turn driven for
deterministic local and content-process execution; continuous background
task-source fairness and automatic delivery while the page is idle are still
future work. The worker surface is intentionally bounded and currently
requires an explicit `ws:`/`wss:` URL. Shared/service/worklet workers,
transferables, complete worker Web IDL parity, and the remaining native/CDP
replacement gates remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_worker_drives_websocket_text_binary_and_close_events --locked -- --exact` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine worker --locked` (16 passed)
- `git diff --check`

The evidence is local-only. Remote CI, release, registry publication, and
final native/CDP parity or production-promotion claims remain pending the
wider issue #40 gates.
