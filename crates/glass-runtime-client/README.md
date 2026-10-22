# glass-runtime-client

A small Rust subprocess client for the optional `glass-core runtime` interface.
It depends on serde_json and the standard library. It does not link GC's engine
or terminal UI.

Use a path dependency from a local checkout, or a Git dependency pinned to a
reviewed glass-core commit. The executable is installed separately.

```rust,no_run
use glass_runtime_client::{RuntimeClient, json};
use std::time::Duration;

let mut runtime = RuntimeClient::spawn("gc", "/absolute/workspace")?;
let initialize = runtime.request("initialize", json!({
    "protocolVersion": 1,
    "storageDir": "/absolute/private/gc-sessions"
}))?;

loop {
    if let Some(message) = runtime.recv_timeout(Duration::from_millis(25))? {
        if message["id"] == initialize {
            // Check result/capabilities before creating a session.
            break;
        }
    }
}
# Ok::<(), std::io::Error>(())
```

After initialization, send `session/new` with a workspace and host tool schemas.
Submit `turn/prompt` and keep polling. Handle `runtime/event` notifications and
`tool/execute` requests while waiting for the prompt response. Apply your own
permission checks before calling `tool_result`. A question tool may use the same
callback to wait for a user answer. Use `request` to send steering or cancellation
without waiting for the active prompt to finish.

The client keeps a bounded reader queue. Drain it continuously to avoid blocking
the runtime through pipe backpressure. Dropping the client kills and reaps its
owned child; it does not cancel tools executed by the host. For graceful closure,
cancel and settle any active turn, then send `shutdown`.

The [protocol reference](../../products/glass-core/docs/reference/runtime.md) defines messages,
ownership, limits, persistence, and error handling. The
[Python reference host](../../products/glass-core/examples/runtime_host.py) demonstrates a complete
conversation without requiring a GUI or terminal framework.

## Typed host operations

The `protocol` module provides SessionOptions, ToolDefinition, TurnInput,
ContextBlock, ImageInput, ToolResult, RuntimeEvent, ToolRequest, and RpcError.
`recv_typed_timeout` separates events, tool requests, and correlated responses.
Unknown event names remain strings so additive capabilities do not break hosts.

Convenience methods cover initialization, sessions, typed prompts and steering,
follow-ups, cancellation, incremental entries, structured results, progress,
cancellation acknowledgements, reconciliation, models, and isolated completion.
The raw request interface remains available for other controls.

Stable entry IDs identify append-only journal byte positions. Use the returned
cursor for incremental reads. Session manifests retain execution ownership;
resume_session requires an expected ToolMode and does not need the original
creation parameters.

To run the actual subprocess client test after building GC:

```sh
GLASS_CORE_TEST_BINARY="$PWD/target/core-release/glass-core" \
  cargo test -p glass-runtime-client --test runtime -- --ignored
```
