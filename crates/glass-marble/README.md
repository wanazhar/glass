# glass-marble

Self-hosted durable collaborator built on Glass Core.

Marble is a product surface, not another engine. It runs a `glass-core runtime`
subprocess, negotiates protocol v2 mixed tool ownership, and keeps the parts a
human interacts with: conversations, a durable transcript, approvals, live turn
events, and a small HTTP/JSON surface that both the WebUI and the iOS client
consume.

## What it owns

- **Mixed sessions**: Core executes its native coding tools; only `glass.*`
  integrations are routed back to Marble over `tool/execute`.
- **Governed tools**: `glass.workspace.list`, `glass.workspace.read`, and
  `glass.workspace.write`, each resolved against one canonical workspace root.
- **Marble's own control layer**: posture (`observe`, `workspace`,
  `unattended`), denied path rules, revocable standing grants, a FIFO admission
  queue with pause and stop, and an audit ledger of every governed decision.
- **Durable product state**: a revisioned `product-state.json` written
  atomically under the data directory.
- **Turn events**: an ordered, bounded buffer with live subscribers, replayed
  from a cursor after a reconnect.
- **Access control**: loopback by default, an access token off loopback, origin
  checks for mutations, and bounded request bodies.

## Use

```text
cargo run -p glass-marble -- "summarise the workspace"
cargo run -p glass-marble -- serve --port 4783 --profile workspace
```

Serving off loopback requires a token:

```text
glass-marble serve --host "$(tailscale ip -4)" --token-file ~/.marble-token
```

`GLASS_CORE_BINARY` selects the Core binary; `GLASS_MARBLE_DATA_DIR` selects the
data directory.

The full product guide, HTTP surface, Tailscale setup, and the iOS client are
documented in [docs/marble.md](../../docs/marble.md).

## Checks

```text
cargo check -p glass-marble --all-targets --locked
cargo test -p glass-marble --locked
```

Adapter tests use `tests/fixtures/fake_glass_core_runtime.py`, so they need
`python3` and no provider account.
