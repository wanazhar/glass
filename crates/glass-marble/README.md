# glass-marble

An always-on, self-hosted personal assistant built on Glass Core.

Marble is the assistant itself, not a surface for something else. It keeps its
own conversations, transcript, approvals, posture, memory, routines, presence,
and notification channels, and it stays reachable on the machine and tailnet you
already own. Core is the engine underneath: Marble runs a `glass-core runtime`
subprocess, negotiates protocol v2 mixed tool ownership, and owns everything a
person touches.

## What it owns

- **Mixed sessions**: Core executes its native coding tools; only `glass.*`
  integrations are routed back to Marble over `tool/execute`.
- **Governed tools**: `glass.workspace.list`, `glass.workspace.read`,
  `glass.workspace.search`, and `glass.workspace.write`, each resolved against
  one canonical workspace root.
- **Marble's own control layer**: posture (`observe`, `workspace`,
  `unattended`), denied path rules, revocable standing grants, a FIFO admission
  queue with pause and stop, and an audit ledger of every governed decision.
- **Presence**: what it is doing now, uptime, and what a restart cost it.
- **Memory**: facts you asked it to keep, and suggestions it waits for you to
  confirm. Facts are carried into every turn.
- **Routines**: saved prompts on a schedule that fire through the same queue.
- **Notifications**: a JSON webhook, and APNs push for the iOS app.
- **Voice, on device only**: the host shapes a reply for speech and never sees
  audio.
- **Durable product state**: a revisioned `product-state.json` written atomically
  under the data directory.
- **Turn events**: an ordered, bounded buffer with live subscribers, replayed
  from a cursor after a reconnect.
- **Access control**: loopback by default, an access token off loopback, origin
  checks for mutations, and bounded request bodies.

## Use

```text
cargo run -p glass-marble -- "summarise the workspace"
cargo run -p glass-marble -- serve --port 4783 --profile workspace
```

Keep it running after you log out:

```text
cargo run -p glass-marble -- install-service --host "$(tailscale ip -4)" --token-file ~/.marble-token
```

That prints the systemd user unit (Linux) or LaunchAgent (macOS) and the commands
that enable it. Add `--write` to install it.

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
cargo clippy -p glass-marble --all-targets -- -D warnings
```

Adapter tests use `tests/fixtures/fake_glass_core_runtime.py`, so they need
`python3` and no provider account.
