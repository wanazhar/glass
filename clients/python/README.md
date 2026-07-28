# Glass Python thin client

`glass_client.py` uses only the Python standard library. It starts an absolute
Glass binary, negotiates MCP, and provides `navigate`, `observe`, `click`,
`verify`, `batch`, `workflow`, and `wait` helpers for Python automation programs, including
guarded form filling. All targeted mutation helpers accept optional revision
guards.

```python
from glass_client import GlassClient

glass = GlassClient(command="/absolute/path/to/glass")
try:
    glass.navigate("https://example.com")
    print(glass.observe_semantic("structured"))
    print(glass.workflow({
        "schemaVersion": 1,
        "name": "read-title",
        "workflowVersion": "1.0.0",
        "inputs": {},
        "budgets": {"maxSteps": 1, "maxDurationMs": 10000, "maxRetries": 0, "maxExtractedBytes": 8192},
        "steps": [{"id": "observe", "action": "observe", "transaction": "read_only"}],
        "terminalCondition": {"titleContains": "Example"},
        "outputs": {},
    }))
    # Pass checkpoint=... to resume only its reconciled safe suffix.
finally:
    glass.close()
```

To connect to a running Linux or macOS daemon, pass
`daemon_socket="/path/to/glass.sock"` instead. Inspect the negotiated
`localDaemon` capability, then call `acquire_mutation_lease()` before mutation
helpers; the client adds the lease token to subsequent calls.

Install the thin client from this directory with `python -m pip install .`.

The client accepts newline-delimited and `Content-Length` MCP frames and caps
individual frames at 4 MiB.

`glass.initialize()` negotiates Glass schema versions and returns the
`GlassCapabilityManifest`. The same manifest is stored at
`glass.capabilities`; callers should inspect its capability flags before using
optional operations.
