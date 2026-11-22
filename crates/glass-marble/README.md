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

## Updating an existing source-checkout host

The host binary is `glass-marble`, not `glass` or `glass-core`. The installer
records the absolute path of the executable used to run `install-service`:
the documented `cargo run` installation therefore normally points at
`target/debug/glass-marble`. Check the existing definition before rebuilding:

```sh
# Linux: inspect ExecStart
systemctl --user cat dev.glass.marble.service
# macOS: inspect ProgramArguments[0]
plutil -p "$HOME/Library/LaunchAgents/dev.glass.marble.plist"
```

From the checkout, on `fix/marble-round2`, for a service using `target/debug`:

```sh
git pull --rebase
cargo build -p glass-marble --locked
systemctl --user restart dev.glass.marble.service  # Linux
# macOS, instead of systemctl, for an already loaded login agent:
launchctl kickstart -k "gui/$(id -u)/dev.glass.marble"
```

For a service using `target/release/glass-marble`, build with `--release`
instead. For a Cargo-installed host at `~/.cargo/bin/glass-marble`, replace the
build command with `cargo install --path crates/glass-marble --locked --force`.
For a manually copied executable, stop the service, replace that executable
with the new build, then start it. Restarting a foreground host means stopping
the old process and repeating its original `serve` invocation with the same
workspace, data directory, token, and engine settings. Rebuilding alone does
not update a running process. No unit rewrite or `daemon-reload` is needed if
only the executable changed.

Marble state/settings remain in `--data-dir` / `GLASS_MARBLE_DATA_DIR`, defaulting
to `$XDG_DATA_HOME/glass-marble` or `~/.local/share/glass-marble`. Core provider
configuration is separate: `$XDG_CONFIG_HOME/glass-core/config.toml` or
`~/.config/glass-core/config.toml`. Preserve the existing service environment;
Core is selected by `--core-binary`, `GLASS_CORE_BINARY`, or `glass-core` on PATH.

Confirm the running host (use its configured host, port, and HTTP/HTTPS scheme):

```sh
curl -sS http://127.0.0.1:4783/healthz | python3 -c 'import json,sys; print(json.dumps(json.load(sys.stdin)["build"]))'
git rev-parse HEAD
```

`build` is `{ "version": "0.3.14", "gitCommit": "<full SHA>",
"gitCommitShort": "<12-character SHA>" }`; compare `gitCommit` to the checkout's
HEAD. `/healthz` needs no token and includes the build even when health is 503.
The authenticated SSE `hello` event adds the same `build` object next to its
existing `cursor`. `glass-marble --version` prints the version and both SHAs,
but checks the executable on disk, not the running service. Git metadata is
captured at build time; source archives or builds without Git report `unknown`
for both SHAs.

## Optimistic sends

Chat sends (`POST /api/turns`, `/api/everyone`, and `/api/prism/dispatch`) accept
`client_message_id` or `clientMessageId`. IDs must be 1–128 printable ASCII
characters (spaces allowed); invalid IDs, including conflicting aliases, return
`400 invalid_client_message_id` without sending. IDs are scoped to the chat,
not the thread. New default chats use an initial-send scope until the engine
assigns their conversation ID; first named sends also retain their initial
Marble scope. Reusing an ID returns the original successful response without
another send, even after restart. Omit the field for the existing behavior.

Successful responses add `client_message_id` and `entry_id` (the stable user
entry ID), including Everyone multi-agent responses. User entries expose
`client_message_id` in history, thread detail, and `entry` SSE event data. The
host saves the send receipt before returning success. If it restarts during
admission before a response was saved, retry returns `409 client_message_pending`
rather than risk executing the message twice.

## Everyone settlement

Everyone reserves one `kind: "pending"` transcript row per addressed agent before
dispatch. Its `id`, position and `at` remain unchanged when it becomes an
`assistant`, `failure`, or cancellation `system` row. `entry` SSE events announce
pending rows and their assigned task IDs; clients must upsert by entry ID, not
append a second row on `reply`. Failure replies include the entry's `failure`
object. Existing tool rows and lifecycle events remain separate.

Busy named agents wait in the persisted admission queue instead of losing their
answer. `answering` includes queued agents; `busy` reports unavailable admission
(for example, no pool or a full queue). Each engine sees its own durable history,
not other agents' current or previous Everyone replies. A thread adds only its
parent message. Successful earlier answers, including incorrect ones, remain in
that agent's history. A latest failed request retries from its pre-input checkpoint.

Before any tool has started, transient generation failures (network/provider
timeouts and HTTP 429/502/503/504) retry once automatically for Everyone. The
logical task/user/reply entry identities remain stable; the second Core attempt
uses a checkpoint fork and a distinct durable submission. `task_retry` reports
`attempt: 1`, `phase: "generation"`, and `reason: "transient_failure"`. Cold
conversation creation also retries transient admission once. Provider rejections,
empty replies, and exhausted retries become structured failures with manual
`POST /api/turns/retry` available. `failure.retryable` continues to describe manual
Retry availability, not automatic replay eligibility. Completed tool side effects
and an unresponsive host-level runtime deadline are not automatically replayed;
they require an explicit Retry rather than risking duplicate side effects.

## History paging

These transcript endpoints accept `limit` and an exclusive `before` entry-ID cursor:

- `GET /api/everyone?limit=50&before=<entry-id>`
- `GET /api/transcript?conversationId=<chat-id>&limit=50&before=<entry-id>`
- `GET /api/threads/<thread-id>?limit=50&before=<entry-id>`

The first request omits `before` and gets the latest N entries. Each page retains
transcript order (oldest to newest within that page). Prepend older pages without
reversing them. IDs, not timestamps, avoid gaps at equal timestamps and keep a
pending row's cursor valid when that row settles. Upsert SSE changes by ID;
timestamp `after` polling alone does not detect updates to existing entries.

Every response adds `hasMore` and `nextBefore` (the oldest returned entry ID when
older entries exist, otherwise `null`). Pass `nextBefore` back as `before`.
Thread metadata stays under `thread`; the parent entry appears only on the page
containing it, not as a redundant copy on every page. Existing entry fields and
transcript `cursor`, `omitted`, `revision`, and `conversationId` remain unchanged.

Defaults are backward compatible: without paging parameters Everyone and thread
detail still return all retained entries; `/api/transcript` still returns the
latest 500. `before` without `limit` uses 500. Explicit limits clamp to 1–1000;
non-integer/overflowing limits return 400 `invalid_limit`. Empty, control-character
or over-256-byte cursors return 400 `invalid_cursor`; unknown, pruned or out-of-scope
entry IDs return 404 `unknown_cursor`. `before` cannot combine with `after` or
`around` (400 `conflicting_cursors`). Existing `after`, `around`, and `main=1`
transcript behavior is preserved. Keep the same conversation/thread/main filter
while paging; an invalidated cursor requires reloading the newest page.

## Checks

```text
cargo check -p glass-marble --all-targets --locked
cargo test -p glass-marble --locked
cargo clippy -p glass-marble --all-targets -- -D warnings
```

Adapter tests use `tests/fixtures/fake_glass_core_runtime.py`, so they need
`python3` and no provider account.
