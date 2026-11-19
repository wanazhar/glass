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
  one canonical workspace root, plus `glass.crates.list`, which reports the
  workspace's Rust crates, the binaries they ship, and their sibling
  dependencies so a change is scoped to the crate that owns it.
- **Marble's own control layer**: posture (`observe`, `workspace`,
  `unattended`), denied path rules, revocable standing grants, a two-lane
  admission queue with pause and stop, and an audit ledger of every governed
  decision. Typed messages run before waiting routine firings.
- **Presence**: what it is doing now, uptime, and what a restart cost it.
- **Memory**: facts you asked it to keep, suggestions it waits for you to
  confirm, and in-place edits. Facts are carried into every turn.
- **Corrections**: a running turn can be steered with a new instruction, keeping
  its context instead of stopping and starting over.
- **Routines**: saved prompts on a schedule that fire through the same queue,
  count the runs a sleeping host missed, and catch up once on wake.
- **Notifications**: a JSON webhook and APNs push, with retries, duplicate
  suppression, quiet hours, and re-alerts until an approval is answered.
- **Voice, on device only**: the host shapes a reply for speech and never sees
  audio.
- **Model choice**: the model new conversations use, visible and changeable
  without restarting the service.
- **History you can read and keep**: lean state plus a cursor-paged transcript,
  transcript search, conversation rename/delete, JSON export, and one daily
  backup beside the state file.
- **Durable product state**: a revisioned `product-state.json` written
  atomically, `fsync`ed, and renamed under the data directory, with bounded
  history; an unreadable document is recovered from the newest daily backup
  instead of starting empty, and `glass-marble restore` puts a backup or export
  back in place.
- **Crash-durable turns and prisms**: every running turn journals its owner and
  partial reply under `live/` at a bounded rate, so a restart keeps what a person
  already watched arrive, marks the task `interrupted`, and explains it once; a
  marble caught mid-dispatch restarts `blocked` with its conversation intact
  instead of looking busy forever.
- **One chat, any surface**: the web UI and the iOS app share the document,
  transcript pages, and turn intake, and a marble's conversation is routed back
  to the engine that owns it, so it continues from either surface.
- **Supervision**: a dead engine transport is replaced in place before the next
  turn, a turn that lost it settles `failed` with the reason, and presence keeps
  the restart count and the last error.
- **One instance per data directory**: `serve` and `restore` take a lock file
  and refuse a second copy by pid instead of overwriting the first one's
  history.
- **Honest health**: `/healthz` reports the engine, the document, backup age,
  quarantined documents, and the last write error, answers `503` when any of it
  is wrong, and `glass-marble doctor` checks the same ground from the terminal.
- **A busy host queues honestly**: a message that arrives while a turn is
  running is answered `202` with its place in line, not a gateway timeout, and
  a routine firing that is bumped by a typed message is reported in presence.
- **Recovery on demand**: repeat the last failed or interrupted turn, and hold
  scheduled routines inside a quiet window.
- **Cheap polling**: `/api/state` revalidates with an `ETag`, responses keep the
  connection alive, connections are capped, and login failures are throttled.
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
