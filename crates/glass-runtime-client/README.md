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

The [protocol reference](../../docs/reference/runtime.md) defines messages,
ownership, limits, persistence, and error handling. The
[Python reference host](../../examples/runtime_host.py) demonstrates a complete
conversation without requiring a GUI or terminal framework.
