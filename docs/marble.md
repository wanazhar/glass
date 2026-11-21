# Glass Marble

[Documentation index](INDEX.md) · [Glass Core resident harness](glass-core-harness.md) ·
[Host runtime protocol](../products/glass-core/docs/reference/runtime.md)

Glass Marble is an always-on, self-hosted personal assistant. It is the
assistant itself, not a surface for something else: it keeps its own
conversations, transcript, approvals, posture, memory, routines, presence, and
notification channels, and it stays reachable on the machine and tailnet you
already own.

Core is the engine underneath. Marble runs a `glass-core runtime` subprocess,
negotiates protocol v2 mixed tool ownership, and owns everything a person
touches: what it is allowed to do, what it remembers, what it does on a schedule,
and how it reaches you when it needs a decision.

The product ships in the workspace as the `glass-marble` crate, with the phone
client in `apps/glass-marble-ios`.

## The shape of the assistant

| Piece | What it does |
| --- | --- |
| Conversations and transcript | Durable, revisioned, replayable from a cursor |
| Control layer | Posture, denied paths, grants, pause queue, audit ledger |
| Presence | What it is doing now, how long it has been up, what a restart cost |
| Memory | Facts you asked it to keep, plus suggestions you confirm |
| Routines | Saved prompts on a schedule that fire without you |
| Notifications | A webhook and APNs push when it needs you |
| Voice | On-device speech only; nothing is sent to a provider |
| Surfaces | One responsive WebUI and one iOS app over the same JSON API |

## Always on

An assistant that only exists while a terminal is open is not always on. Marble
is designed to be installed as a login service and left alone:

```text
glass-marble install-service --host "$(tailscale ip -4)" --profile workspace --token-file ~/.marble-token
```

That renders a systemd user unit on Linux or a LaunchAgent on macOS with exactly
the arguments you used interactively, then prints the two commands that enable
it. Add `--write` to install it; without it nothing on disk changes, so the
definition can be reviewed first. On Linux, `loginctl enable-linger "$USER"`
keeps it running after you log out.

### Supervision, one instance, and honest health

Marble stays up by itself. The service definition restarts the process, and
inside it the turn worker supervises the engine: when the `glass-core runtime`
transport dies, the next turn first replaces the engine in place — same
workspace, same sessions directory, same negotiated capabilities — and then
runs. A turn that lost its engine settles `failed` with the reason in the
transcript rather than looking like it is still working. Presence carries a
`runtime` block (`health`, `restarts`, `lastRestartAt`, `lastError`), kept after
a successful restart so a flapping engine is visible rather than forgiven.

`serve` and `restore` both take a lock file (`marble.lock`) in the data
directory. A second copy is refused with the owning pid instead of silently
rewriting the first one's document, and a lock left by a process that is gone is
taken over rather than requiring a manual delete. `restore` therefore cannot
write underneath a running service.

`GET /healthz` is the one place that answers "is this host actually working":
version, pid, revision, event sequence, posture, waiting and running turns,
recovered tasks, the runtime block, the data directory with its document, backup
count and age, quarantined documents, and `lastPersistError`. It answers `200`
only when the engine is answering and the last durable write succeeded, and
`503` otherwise.

The installed service writes `marble.log`, so the installer prints a rotation
reminder. An always-on assistant writes forever, and an unbounded log is the one
thing that fills a small host's disk without warning.

`glass-marble doctor` checks the same ground without changing anything: data
directory writability, the document and its revision, the newest backup's age,
quarantined documents, whether another instance holds the lock, the engine
binary on `PATH`, the sessions directory, and whether a token is configured. It
exits non-zero when a check needs attention, so it is safe to run from a timer.

### Presence

`GET /api/presence` answers what the assistant is doing: `working`, `waiting`
(an approval is pending), `paused`, or `idle`, plus uptime, the active task, the
queued count, the machine name, how many routines are scheduled, and how many
memories it holds. The same document is embedded in `GET /api/state` and
summarised in `/healthz`, so both surfaces and any script agree.

### Crash recovery

Durability is the product, not a feature on top of it. Three layers make a kill
mid-turn survivable, and each one answers a different question:

- **Core's session journal** is what makes a conversation resumable. Marble does
  not copy it. Each engine keeps its journal under the data directory
  (`sessions/`, and `sessions/prism/<marble>/` for a marble's own engine), so a
  conversation survives a restart of Marble and of Core with its context intact.
- **The live journal** is what keeps the reply being written. `product-state.json`
  only learns about a reply when the turn settles, so a crash in between used to
  lose every word a person had already watched arrive. Each running turn keeps a
  small record under `live/`, rewritten at most every 250 ms with who is running
  it, for which conversation, and the text so far. On startup each record is
  folded back in: the partial reply is appended to its conversation, the task is
  marked `interrupted`, and a system entry says the host restarted mid-turn and
  how far the reply got. The record is then removed, so a recovery can never
  replay as a second transcript entry. An unreadable record is kept aside as
  `*.corrupt-<ms>` instead of being dropped.
- **The document** records the outcome. On startup, every task still marked
  `running` or `queued` is marked `interrupted`, a system transcript entry
  records what happened, the routine that owned it is marked `interrupted`, and
  presence reports `recoveredTasks`. Send the message again or retry to continue.

The record is also the intent: a turn that was admitted and never wrote a word
still leaves a visible interrupted task instead of disappearing. The same is true
of a **Prism** marble — one caught mid-dispatch restarts `blocked`, keeps its
conversation, says why on its roster row, and is dispatchable again instead of
sitting `working` forever behind a refused dispatch.

Writes are durable past the process: the document and the live records are
written to a temporary file, `fsync`ed, renamed over the old bytes, and the
parent directory is synced. That is the difference between surviving `kill -9`
and surviving a power loss.

### Switching surface

A chat belongs to the host, not to the app that opened it. The web UI and the
iOS app read the same document, page the same transcript, and post to the same
`POST /api/turns`, so opening either one lands on the conversation list the other
already has, with the same titles, tasks, and history.

A marble's conversation is answered by the engine that created it. Posting a
message into one from a chat surface routes it to that marble's engine rather
than to the chat engine, which cannot see that session: the same conversation
continues, from the roster, the phone, or the browser, and it runs beside the
chat engine instead of waiting behind it.

### Retention

Months of turns must not turn into a state file that every response has to parse.
The product document is bounded: 400 transcript entries per conversation, 4,000
transcript entries overall, 500 settled tasks, and 100 conversations by most
recent activity. Work that has not settled — `queued`, `running`, or waiting for
approval — is never trimmed. A heartbeat tick folds the document back inside the
bounds, startup does the same once, and `GET /api/state` reports the limits plus
`trimmedTranscript`, `trimmedTasks`, `trimmedConversations`, and `lastTrimAt`, so
the trade-off stays visible instead of silent.

## Ownership boundary

Marble uses protocol v2 mixed tool ownership:

- Core executes its own coding tools inside the configured workspace and under
  Core's policy.
- Only `glass.*` integrations registered by Marble come back to Marble over
  `tool/execute`. Those are the tools a human approves.
- A runtime that reports no `mixedTools` capability is rejected at startup
  instead of silently degrading to a host-only session.

The governed host catalog is deliberately small:

| Tool | Effect |
| --- | --- |
| `glass.workspace.list` | List one workspace directory |
| `glass.workspace.read` | Read a UTF-8 workspace file, bounded and truncated |
| `glass.workspace.search` | Find text across the workspace with file and line numbers |
| `glass.workspace.write` | Write a workspace file, creating parents |
| `glass.crates.list` | Read the workspace's Rust crates, their binaries, and their sibling dependencies |

Every path is resolved against one canonical workspace root. Absolute paths,
parent traversal, and symlinks that escape the root are refused before any
filesystem call. Search skips `.git`, `.ssh`, `.env*`, dependencies, and build
output, so a wide query cannot become a secret-read.

## Prism protocol: several marbles at once

One marble is one assistant, and one engine process runs one turn at a time.
That is the right shape for a person talking to an assistant, and the wrong one
for handing the same question to four specialists at once. The **Prism
protocol** is the second shape: a roster of named marbles, dispatched together
like a hive leaving the hive.

```text
GET  /api/prism
POST /api/prism/marbles                        {"name", "role", "glyph", "hue", "workspace"}
POST /api/prism/marbles/{id}/update
POST /api/prism/marbles/{id}/remove
POST /api/prism/dispatch                       {"text", "marbles": [ids]}
```

A marble is a name, a role, a shape (`circle`, `hex`, `shield`, `drop`, `star`,
`square`), a hue, and one optional workspace subdirectory inside the host
workspace. It keeps its own conversation, so coming back to it tomorrow is
coming back to the same marble rather than to a new thread. The roster holds at
most eight, because a roster is meant to be read at a glance rather than paged.

What stays shared is the product document, the event bus, the approval inbox,
the cancel table, and the control layer. What is per marble is identity,
conversation, workspace root, and the engine process: Core's runtime protocol
serialises turns inside one process and tells hosts to use separate processes
for concurrent agents, so Prism starts an engine per marble on the first
dispatch and leaves a host that never swarms paying nothing.

Discipline that keeps a swarm legible:

- A marble takes one turn at a time. Asking a busy marble answers `409` with
  its name instead of parking the work out of sight.
- A dispatch is refused while admission is paused, because a swarm would
  otherwise be the one kind of work that ignores the switch.
- Every dispatched turn lands in the same durable task table and cancel table as
  a typed message, so `Stop everything` reaches it and presence counts it.
- A missing workspace blocks that marble with the reason on its roster row; it
  does not silently fall back to the host root.
- Removing a marble stops its engine and leaves its conversation in the
  document: leaving a roster is a change of who works, not permission to delete
  what was already said.
- A marble's conversation is continuable from any surface: a typed message for
  it is routed to that marble's engine, and a held-then-resumed message goes the
  same way.
- A restart does not strand a working marble: on startup it becomes `blocked`
  with the reason, keeps its conversation, and can be dispatched again.

Each running turn appears in the roster as `working`; a settled one shows
`done`, a failed one `blocked` with the last error kept on the row. The WebUI
and the iOS app draw a marble as one shape with two eyes and carry its state in
motion on that same object, so a roster can be read without reading every name.

## Tooling

A marble reaches for two kinds of tool, and the difference matters to anyone
deciding how much to trust it.

- **Core-native tools.** Core executes its own coding tools inside the mixed
  session, under Core's policy. Marble cannot approve them, so it does not
  claim to. It records every `tool_start` and `tool_end` it observes, keeps the
  newest 200 names with run and failure counts, and lists them.
- **Governed integrations.** The `glass.workspace.*` tools are executed by Marble
  itself, and every call is classified by the control layer before Core sees a
  result.

`GET /api/tools` reports both owners, the posture in force, and what the engine
has actually used:

```json
{
  "protocol": "mixed",
  "profile": "workspace",
  "core": { "nativeTools": true, "observed": [{"name": "read_file", "runs": 3, "errors": 0, "lastAt": 1763500000000}] },
  "governed": [{"name": "glass.workspace.write", "owner": "marble", "effect": "write"}],
  "counts": { "governed": 4, "observed": 1 }
}
```

The Control panel lists that inventory as it happens, so "Marble can read and
write files" becomes "Marble wrote `src/main.rs` at 14:02, and you approved it".

### Working across Glass crates

Glass is several crates that only mean something together: Core owns the model
loop, `glass-runtime-client` speaks its protocol, `glass-browser` owns the
browser, `glass-dev` owns the development workspace, and `glass-mod-api` is the
extension boundary. A marble asked to change behaviour sees file paths, and a
path does not say which crate owns what it found.

So Marble reads the workspace manifests and keeps the map:

```json
{
  "workspace": { "version": "0.3.14", "members": 8, "marble": "glass-marble" },
  "crates": [
    {
      "name": "glass-browser",
      "crateName": "glass_browser",
      "version": "0.3.14",
      "path": "crates/glass-browser",
      "binaries": [{ "name": "glass-browser", "onPath": null }],
      "dependsOn": ["glass-browser-core", "glass-tui"],
      "reach": []
    }
  ],
  "edges": [{ "from": "glass-browser", "to": "glass-browser-core" }]
}
```

`reach` is the honest part. `library` means this host links the crate, because
the marble's own manifest says so; `binary` means the crate ships an executable
that resolves on `PATH` right now. An empty `reach` means Marble has no direct
route and the map is telling it to go through the crate that does, instead of
inventing an import.

Four surfaces, one document:

- every conversation is briefed with the crate names and told to ask
  `glass.crates.list` rather than infer ownership from a path;
- `glass.crates.list` answers mid-turn, with `name` for one crate and what
depends on it, so a change can be scoped before it is written;
- `GET /api/crates` serves the same map to a surface, fetched when someone
  opens the sheet that shows crates rather than on every state poll: the map is
  a workspace scan, and a poll should not pay for a document nobody is reading.

The manifests stay the source, so a crate added to the workspace appears without
a code change, and a member that cannot be read is simply absent rather than
invented.

## Marble's control layer

Marble does not inherit the engine's idea of what is allowed. Every governed
call is classified by the product's own layer first, and the answer is journalled
before Core ever sees a result. The layer has four parts.

### Posture

One profile applies to the whole instance, selectable from the WebUI, the iOS
app, or `--profile`:

| Profile | Reads | Writes | Approvals |
| --- | --- | --- | --- |
| `observe` | Allowed | Refused | None asked |
| `workspace` (default) | Asked unless granted | Asked unless granted | Per call |
| `unattended` | Allowed | Allowed | None asked |

Denied path rules apply to every posture, including `unattended`, and an
approval cannot override them.

### Denied paths

The default rules refuse `.git`, `.ssh`, `.env`, and `node_modules` as whole path
components, so `.env.local` is refused while `.gitignore` is not. A refusal names
the rule, which is what the model receives as its tool error, so it can adapt
instead of retrying the same call.

### Grants

"Always allow" on an approval records a standing grant for that tool in that
directory. Grants carry a use count, cover only that exact scope, never leave the
workspace, and can be revoked from either surface. A grant answers the repeated
question, not every question: a write in a different directory still asks.

### Queue, pause, and stop

Admission is Marble's decision too:

- `Pause queue` holds new messages in a durable FIFO queue instead of admitting
  them. `Resume queue` hands each held entry back to its lane, so a typed message
  still runs before a held routine firing. A held firing resumes as a firing: its
  run was counted when admission held it, so resuming only records the task that
  carries it, and settlement still reaches the routine. A resumed entry keeps a
  durable `queued` row while it waits in its lane, so it stays visible until the
  engine admits it. A firing whose routine is removed or disabled while it waits
  is skipped when its turn comes instead of running work nobody asked for, and
  the skip is published with the routine id.
- `Stop everything` pauses admission and cancels every running task in one move.
  Both are reversible: the pause switch and the queue survive a restart.
- A stop reaches governed calls as well as the model loop. A call parked on an
  approval is refused at once, and a call that is already running stops being
  waited on: the runtime is told the call was stopped and the task is aborted
  without waiting for a slow search or write to report back. The executor itself
  is not preempted mid-call, so a write already in flight may still land, and the
  ledger records the call as denied with that reason.
- Routines are admitted through the same queue, so a paused assistant queues its
  scheduled work instead of running it. Running a disabled routine by hand is a
  deliberate override: it still runs, and the answer says it overrode the pause.
- Typed messages and routine firings share the queue but not the priority: a
  message you type runs before any routine firing that is waiting, whether the
  scheduler fired it or you ran it by hand. Presence counts both lanes and says
  which one the worker takes next.
- The control panel's queue lists every waiting turn, tagged `typed` or
  `routine`, with the reason it has not started: behind how many typed messages,
  next in the routine lane, or held because admission is paused. A routine held
  by pause names the routine that fired it, so queued work is never anonymous.

### Several chats at once

A conversation belongs to the engine that created its session, so Marble keeps a
small pool of lanes instead of one serial worker. Each lane is its own runtime
process with its own durable store, and the lane a conversation was admitted on
is recorded on the conversation record, so a later turn or a correction returns
to the same engine. Three lanes are available: a person working a question, a
fix, and a review does not have to wait for the first answer to finish. Within
one conversation turns stay serial and in arrival order, and a busy lane keeps
its queue, so two messages to the same chat never interleave.

- Lane zero keeps the existing `sessions` directory, so conversations written
  before lanes existed still open. Later lanes keep their store under
  `sessions/lanes/<n>`.
- A new chat goes to the lane with the fewest turns ahead, and ties rotate, so
  two fresh chats do not land on the same engine.
- Corrections are routed to the lane that admitted the conversation, never
  broadcast: a steer aimed at one chat cannot reach another chat's engine.
- A poll that finds a running turn in a different conversation does not make the
  chat on screen look busy, and live text from another lane is never typed into
  the transcript being read.
- Workspace writes are coordinated per path. A chat that read a file is refused
  a write once another chat (or an editor) changed it, and a chat that never
  read a file cannot replace one another chat just wrote; the refusal tells the
  model to re-read. The approval card names the other chat before the person
  decides, and writes to one path are serialised, so two lanes never interleave
  inside a file.

### Audit ledger

The last 200 decisions (`allowed`, `approved`, `denied`) keep their tool, path,
conversation, task, and reason, so a surface can explain why a tool ran or did
not. The ledger is part of the durable product document.

One-shot runs have no surface to answer an approval, so `--approve` maps to an
approving host posture and the default refuses. Serving waits
`--approval-wait-secs` (default 600) for a human before a governed call fails
closed.

## Memory

Memory is deliberately legible: a short list of statements you asked Marble to
keep, plus suggestions it offers when it notices something durable.

Say any of these in a message and Marble writes the fact itself, without a model
turn:

```text
remember this: the deploy key lives in 1Password
remember that: I prefer squash merges
update memory: my timezone is Asia/Kolkata
save to memory: the office wifi is glassnet
forget: the office wifi
```

Facts are carried into every turn as context, so they are used rather than merely
stored, and each fact counts its uses. A statement Marble notices ("I prefer…",
"my name is…", "I work…") waits as a suggestion with Save and Dismiss until you
decide. Nothing is remembered silently, and `forget` removes it immediately.

A fact can also be corrected in place: `POST /api/memory/{id}/update` with the
new `text` (and optionally `kind`) keeps the same id and use count instead of
forgetting and re-adding it. Both surfaces offer Edit next to Forget, so a fact
that has drifted out of date is one edit away rather than a loss.

Limits are explicit: 200 facts, 280 characters each, 8 suggestions waiting.

## Correcting a running turn

Stopping a turn to fix a wrong assumption throws away everything it has already
read. Core's `turn/steer` interrupts the current request, keeps the context, and
continues the same turn with the correction. Marble exposes that as:

```text
POST /api/turns/steer
{"text": "use src/lib.rs, not src/main.rs", "taskId": "task-…", "conversationId": "conv-…"}
```

The route resolves the request against the durable task table before handing it
to the worker: the target must be a task whose status is `running`, so a
correction can never land on a different turn than the one you were watching. An
idle host answers `409 no_running_turn`. Accepted corrections are published as
`steer_queued`, then `steered` when Core accepts them, and `steer_rejected` with
a reason when the runtime refuses.

While a turn runs, the WebUI and the iOS composer show a correction field: text
there goes to the running turn, not a new one. Steer is applied between runtime
frames, so a turn blocked inside a host approval answers the approval first.

The runtime also offers `turn/followUp` for queueing a new turn after the
current one; Marble's own pause queue already covers that, so it is not wired.

## Routines

A routine is a saved prompt plus a small schedule:

```text
every:30m      every:2h      every:1d      daily:09:30
```

The scheduler wakes every 20 seconds, finds the routines that are due, and admits
each one through the same turn channel and pause queue as a typed message, tagged
with the routine that fired it. Minimum interval is 60 seconds, and at most 32
routines are kept. `Run now` fires one immediately from either surface.

Routine work is durable: a restart recomputes the next run from the schedule, and
an interrupted routine run is reported as `interrupted` rather than stuck.

`daily:09:30` means 09:30 where you are, not UTC. The surface that saves a
routine sends its UTC offset, and the host stores one `offsetMinutes` for all
daily routines (`POST /api/routines/offset`, also shown as “Use this device's
time” in the Assistant sheet). Changing it recomputes the next run immediately;
`every:` schedules are unaffected. Both surfaces show the computed next run.

A laptop sleeps and a host reboots, so Marble does not pretend it was awake for
every scheduled run. When a tick finds that occurrences passed while the process
was down, it counts them on the routine (`missedRuns`, `missedLastGap`) and runs
the routine once on that tick, announcing how many runs it caught up on. A
routine with catch-up turned off records the same counts and silently resumes its
schedule instead of starting late work. `POST /api/routines/{id}/catch-up` with
`{"enabled": false}` chooses that, and both surfaces show what was missed.

## Reaching you

Two channels, both optional, both configured from the WebUI or the API. Every
delivery is best effort and happens off the turn's thread: a failed notification
is retried a bounded number of times with a growing delay, then logged, never
allowed to fail or delay a turn.

Reachable is not the same as noisy. Marble keeps the person from being woken by
the same news twice:

- `dedupeSeconds` (default 120) drops a repeat of the same event, conversation,
task, and title inside the window.
- `quietHours` holds routine news and memory prompts inside a local window
(default 22:00–07:00). Approvals and failures always go through, and the WebUI
sends the browser's UTC offset so the host needs no timezone database.
- `reAlertSeconds` (default 300) repeats an unanswered approval up to five
times, and stops as soon as it is decided.
- `retries` (default 2) is the number of extra attempts after a channel fails.

`GET /api/notifications` reports the settings plus `recent`, the last 40 delivery
attempts (and the notifications Marble deliberately held back, with the reason).
The Assistant panel shows the same list under **Recent deliveries**.

### Webhook

Any `http://` or `https://` endpoint that accepts JSON — ntfy, Home Assistant, a
small receiver on your tailnet. Plain HTTP is spoken directly so a tailnet
receiver needs no TLS at all; HTTPS uses `curl`. The payload carries `event`,
`title`, `body`, `conversationId`, `taskId`, and `at`, and an optional bearer
token is sent when configured.

### Apple Push

Token-based APNs with the `.p8` key you already have. Marble builds the short
lived ES256 JWT with `openssl`, then sends it with `curl --http2`:

```json
{
  "apns": {
    "keyFile": "/home/you/keys/AuthKey_ABC123.p8",
    "teamId": "TEAMID1234",
    "keyId": "ABC123XYZ9",
    "topic": "com.wanazhar.glassmarble",
    "environment": "production"
  }
}
```

The key file stays on the host. Device tokens are registered by the iOS app on
`POST /api/devices`; at most 16 devices are kept. If `openssl` or `curl` is
missing, delivery reports that plainly instead of pretending to send.

Subscribed events are `approval`, `settled`, `failed`, `routine`, and `memory`.
`Send test` exercises every configured channel and returns what happened.

## Voice, on device

Marble never sends audio anywhere. The phone transcribes with the on-device
recognizer and speaks with the on-device synthesizer; the host only shapes a
reply into something worth hearing, removing code fences, markup, and URLs
(`POST /api/voice/speak`). The WebUI does the same with the browser's local
speech APIs. Dictation is disabled entirely on a device without offline
recognition rather than falling back to a network service.

## HTTP surface

| Route | Method | Behavior |
| --- | --- | --- |
| `/` | GET | Product UI |
| `/healthz` | GET | Version, revision, posture, waiting and running turns, engine health, document and backup state, last write error; `503` when the engine or the last write is unhealthy |
| `/api/login` | POST | Exchange the access token for an HTTP-only session cookie |
| `/api/state` | GET | Revisioned product document plus presence, memory, routines, voice, notifications, retention; `?lean=1` omits the transcript; revalidates with `ETag` and `If-None-Match` |
| `/api/transcript?conversationId=…&after=…` | GET | Bounded history for one conversation, resumable from a cursor |
| `/api/transcript/search?q=…` | GET | Newest transcript matches across conversations |
| `/api/presence` | GET | What the assistant is doing, uptime, recovery, counts, and the Prism block |
| `/api/tools` | GET | Every tool the marble can reach for: Core-native names it has used, plus the governed integrations and the posture in force |
| `/api/crates` | GET | The workspace's Rust crates with their binaries, sibling dependencies, and how this host can reach each one; `503` when the host was not started in a workspace |
| `/api/prism` | GET | The Prism roster, its limits, glyphs, swarm count, and per-marble state |
| `/api/prism/marbles` | POST | Add one named marble (`name`, optional `role`, `glyph`, `hue`, `workspace`) |
| `/api/prism/marbles/{id}/update` | POST | Change a marble's name, role, shape, hue, or workspace |
| `/api/prism/marbles/{id}/remove` | POST | Take a marble off the roster; its conversation stays |
| `/api/prism/dispatch` | POST | Hand one prompt to several marbles at once (`text`, optional `marbles`) |
| `/api/events?since=N` | GET | Server-sent events with bounded replay from a cursor |
| `/api/turns` | POST | Admit a durable turn (`202` accepted, or `202` queued with its place in line while the worker is busy), queue it while paused, or apply an explicit memory command |
| `/api/turns/steer` | POST | Correct the running turn (`text`, optional `taskId` and `conversationId`) |
| `/api/turns/retry` | POST | Repeat the newest failed, cancelled, or interrupted turn (`taskId` or `conversationId` to choose one) |
| `/api/tasks/{id}/cancel` | POST | Request cancellation of a running task, including one a routine fired or a resume dispatched |
| `/api/approvals/{id}/allow` and `/deny` | POST | Answer one pending governed tool; `{"remember":true}` also records a grant |
| `/api/control` | GET | Control document: posture, pause, grants, denied paths, ledger, queue, and the workspace crate map (`null` outside a Rust workspace) |
| `/api/control/profile` | POST | Select `observe`, `workspace`, or `unattended` |
| `/api/control/pause` and `/resume` | POST | Hold new messages, or dispatch the queue in order |
| `/api/control/stop` | POST | Pause admission and cancel every running task |
| `/api/control/grants` | POST | Add a standing grant (`tool`, `pathPrefix`) |
| `/api/control/grants/{id}/revoke` | POST | Revoke one grant |
| `/api/memory` | GET/POST | Read memory, or remember one statement (`text`, optional `kind`) |
| `/api/memory/{id}/update` | POST | Correct one fact in place (`text`, optional `kind`) |
| `/api/memory/{id}/forget` | POST | Forget one fact |
| `/api/memory/{id}/accept` and `/dismiss` | POST | Decide one suggestion |
| `/api/routines` | GET/POST | Read routines, or add one (`title`, `prompt`, `schedule`, optional `offsetMinutes`) |
| `/api/routines/offset` | POST | Set the UTC offset `daily:HH:MM` is interpreted in |
| `/api/routines/quiet` | POST | Hold scheduled work inside a window (`enabled`, `startMinute`, `endMinute`, `offsetMinutes`) |
| `/api/routines/{id}/run` | POST | Fire a routine now, queueing it when admission is paused; a disabled routine's run answers with `override: true` |
| `/api/routines/{id}/enable` and `/disable` | POST | Pause or resume a routine |
| `/api/routines/{id}/remove` | POST | Delete a routine |
| `/api/routines/{id}/catch-up` | POST | Run the first tick after a gap, or skip to the next run (`enabled`) |
| `/api/notifications` | GET/POST | Read or update webhook, events, APNs, retries, dedupe, quiet hours, and re-alert settings |
| `/api/notifications/test` | POST | Deliver a test notification and report each channel |
| `/api/devices` | GET/POST | List prefixes, or register one APNs device token |
| `/api/devices/{token}/remove` | POST | Forget a device |
| `/api/conversations/{id}/rename` | POST | Rename one conversation (`title`) |
| `/api/conversations/{id}/delete` | POST | Remove one conversation and its settled history; refuses while a turn is running |
| `/api/model` | POST | Choose the model new conversations use (`model`; empty for the engine default) |
| `/api/export` | GET | Portable JSON of the whole document, or one conversation with `?conversationId=` |
| `/api/voice` | GET/POST | Read or set local voice preferences |
| `/api/voice/speak` | POST | Shape a reply into a spoken form |

A turn is answered as soon as the durable task id exists, so a long model turn
never blocks the HTTP surface. Text, reasoning, tool activity, usage, memory
suggestions, routine firings, and settlement stream over `/api/events`; the
transcript itself stays authoritative in the product document.

One worker runs one turn at a time, and its queue has two lanes: messages a
person typed drain first, then routine firings, each lane in arrival order. A
routine that was bumped waits without looking stuck, because presence explains
it. A message that arrives while a turn is running is not answered with a
gateway timeout: the route waits a short bound, then answers `202` with
`"queued": true`, its `position` in its lane, and the `requestId` it will carry.
The message is a durable `queued` task in the document while it waits — counted
as `waitingTurns` in presence, with a `waiting` block that splits `human` from
`routine`, names the `nextLane`, and says why a firing is held — so a restart
does not forget it, and it is replaced by the real task when the engine admits
it. A turn posted while admission is paused is queued the same way, with
`"paused": true`. Approvals fail closed when nobody answers within the
configured wait.

### What a poll costs

A tailnet client polls this surface for months, so the transport is part of the
product. `/api/state` answers with an `ETag` derived from the document revision
and the pending-approval count, and `304 Not Modified` when `If-None-Match`
already matches, so an unchanged refresh costs headers instead of the whole
document. Non-SSE responses keep the connection alive for up to 32 requests
before the client reconnects, which removes a handshake from every poll. The
server accepts at most 64 connections at once and answers `503 busy` beyond
that instead of growing a thread per client, and `/api/login` throttles a caller
after five failures in five minutes so a token cannot be guessed at wire speed.

Persisting a change writes the borrowed document directly; the full clone that
used to happen before each write is gone. A lean poll also drops the transcript
before cloning the document, so the common refresh copies control, presence, and
counts rather than thousands of transcript entries it is about to omit. The
workspace crate map is not part of a poll at all: both surfaces fetch `/api/crates`
when the sheet that shows crates opens, so a poll never pays for a workspace scan.

## Reading history without moving it all

Both surfaces poll `/api/state` on every change, so the full transcript cannot
live in it forever. `GET /api/state?lean=1` keeps every record and count but
returns `transcript: []` with `transcriptCount`, and clients read history from
`GET /api/transcript?conversationId=…` instead. The endpoint accepts `after` (a
millisecond cursor, inclusive so entries sharing a timestamp are not lost) and
returns a `cursor` plus how many older entries it omitted. The WebUI keeps one
conversation's page and appends; the iOS app does the same for the selected
conversation. That keeps a refresh proportional to the conversation in view
rather than to months of history.

History is also something you can curate and take with you:

- rename or delete a conversation from either surface (a running turn blocks
delete);
- search the transcript from the WebUI rail;
- `GET /api/export` writes the whole document, or one conversation with
`?conversationId=`, as JSON with a `format` and `exportedAt`;
- the host writes one backup per day next to the state file under `backups/`
and keeps the newest seven, so a bad edit or a corrupt document is recoverable
without a second service.

## Self-hosting over Tailscale

Marble binds loopback by default. To share it on your own tailnet:

```text
printf '%s' "$(openssl rand -hex 32)" > ~/.marble-token
chmod 600 ~/.marble-token
glass-marble serve --host "$(tailscale ip -4)" --token-file ~/.marble-token
```

Rules the server enforces:

- Serving off loopback without a token is refused at startup.
- Every `/api/*` request needs the token, as a session cookie set by
  `POST /api/login`, an `Authorization: Bearer` header, an `X-Marble-Token`
  header, or a `token` query parameter for scripted access.
- Mutating requests with an `Origin` header must match the `Host` that served
  the page.
- Request heads are bounded and bodies over 256 KiB are rejected before reading.
- Token comparisons run in constant time.

Startup prints the listen address, workspace, data directory, posture, presence,
routine and memory counts, and the notification channels in use, plus the
machine's Tailscale IPv4 address when `tailscale` is installed.

The data directory holds the runtime sessions and the product document
(`product-state.json`). Override it with `--data-dir` or
`GLASS_MARBLE_DATA_DIR`; the default is `$XDG_DATA_HOME/glass-marble`, then
`~/.local/share/glass-marble`.

The document is replaced atomically, so a crash leaves one complete revision.
If it is ever unreadable, startup does not treat that as an empty assistant: it
restores the newest daily backup and moves the bad file aside as
`product-state.corrupt-<millis>.json`. With no backup available it refuses to
start and names the file, so a first write cannot erase what is still there.
Restore deliberately, with the service stopped:

```text
glass-marble restore                      # newest daily backup
glass-marble restore --from ~/marble-export.json  # an /api/export download
```

The document being replaced is copied aside as
`product-state.replaced-<millis>.json`, so a restore is also reversible.

If you need TLS, put Tailscale in front of the loopback listener instead of
terminating TLS in Marble:

```text
glass-marble serve --port 4783
tailscale serve --bg 4783
```

## Web UI

`glass-marble serve` serves one page that adapts from a phone to a wide desktop:
a collapsible conversation rail with transcript search, a transcript that renders
model text as text rather than HTML, live tool activity, approval cards with
Allow, Always here, and Deny, memory suggestion cards, a correction field while a
turn runs, task cancellation, an unlock dialog for token-protected hosts, and two
sheets:

- **Assistant** — presence, remembered facts with Edit and Forget, waiting
  suggestions, conversations with Rename and Delete, routines with
  Run/Pause/Remove, missed-run counts and next run, the model, webhook and APNs
  settings with quiet hours and a test button, registered devices, recent
  deliveries, Export everything, and the read-aloud switch.
- **Control** — posture, pause and stop, grants, denied paths, the tool
  inventory, the queued messages, and the decision ledger.
- **Prism** — the marble roster with a shape, a hue, and a live state per
  marble, a swarm composer, and an editor for adding a marble. A dispatch,
  removal, or refusal is reported inside the sheet, where the person acting is
  looking, rather than only in the header behind it.

Safe-area insets keep the composer above the home indicator, and the layout
honours reduced-motion preferences.

## iOS client

The iOS app lives in `apps/glass-marble-ios` and is built with Xcode, not Cargo.

- SwiftUI surfaces use Liquid Glass (`.glassEffect`) on iOS 26 and later, and
  fall back to `ultraThinMaterial` on earlier systems.
- `Main.storyboard` hosts the first-launch introduction scene through a
  `UIViewControllerRepresentable`; `LaunchScreen.storyboard` is the launch
  screen. Both are compiled from the synchronized `GlassMarble` source folder.
- The host address is stored in `UserDefaults`; the access token is stored in the
  keychain and sent as a bearer token.
- The client reads `/api/state` and consumes `/api/events`, so approvals,
  cancellations, control changes, routine firings, and memory suggestions work
  from the phone exactly as they do in the browser.
- The control screen (shield in the transcript toolbar) switches posture, pauses
  or resumes admission, stops everything, revokes grants, and reads the ledger.
- The Prism screen draws each marble as a shape with two eyes, dispatches a
  swarm, and opens a marble's own conversation. The tooling screen lists what
  Core has used and what Marble governs.
- The memory screen (brain) lists, edits, and forgets facts, saves or dismisses
  suggestions, and holds the read-aloud switch. The routines screen (clock) adds,
  runs, pauses, and removes routines, shows the computed next run and missed
  runs, and can adopt the phone's timezone.
- Conversations can be renamed or deleted from a long press, and the conversation
  list is searchable.
- Tapping an approval or failure notification opens the conversation it came
  from instead of whatever was selected last, including a cold launch.
- Dictation uses the on-device recognizer and the composer's microphone button;
  replies are spoken by the on-device synthesizer.
- The app registers its APNs device token with the host on launch, so a
  notification can reach the phone when an approval is waiting. APNs credentials
  and their `.p8` key are configured on the host.
- `NSAllowsArbitraryLoads` is enabled because a self-hosted tailnet address is a
  user-supplied plain-HTTP origin. Prefer `tailscale serve` if you want TLS.

The project ships one shared scheme, `Glass Marble`, so Xcode and CI build the
same target without a per-machine scheme.

Build and run:

```text
open apps/glass-marble-ios/GlassMarble.xcodeproj
```

Select a development team, choose a simulator or device, and run. The first
launch shows the storyboard introduction; open host settings and enter the
address printed by `glass-marble serve`.

## Build and run

```text
cargo build -p glass-marble --locked
cargo test -p glass-marble --locked
```

One-shot message, one durable turn:

```text
glass-marble "summarise the workspace" --cwd .
```

The one-shot path denies governed tools unless `--approve` is passed, because no
surface is present to answer an approval. Serving takes `--profile observe`,
`--profile workspace` (default), or `--profile unattended`.

Serve the assistant:

```text
glass-marble serve --cwd /path/to/project --port 4783
```

Restore after a bad edit, a corrupt document, or a mistaken deletion. Stop the
service first, because it holds the document in memory and writes it back on the
next change:

```text
glass-marble restore --from ~/marble-export.json
```

## Verification

Local checks for this product:

```text
cargo test -p glass-marble --locked
cargo clippy -p glass-marble --all-targets -- -D warnings
cargo fmt -p glass-marble
```

The adapter tests use `tests/fixtures/fake_glass_core_runtime.py`, so they need
`python3` and no provider account. They cover mixed-session negotiation, tool
execution and denial, per-message durable task identity, live approvals, control
classification and ledger recording, paused admission and in-order dispatch,
typed-message priority over routine firings and the presence block that explains
the wait,
grant lifecycle, token authentication, cross-origin rejection, body limits, event
replay, the HTTP turn route end to end, presence, explicit memory commands,
suggestion decisions, routines and their timezone offset, notification and voice
settings, the local webhook delivery path, lean state, cursor-paged and searched
history, conversation rename/delete, model selection, export, backup recovery,
and restore from a backup or an export file.

`.github/workflows/glass-marble.yml` runs the same checks on Linux and adds the
part a Rust-only loop cannot: a signing-free simulator build of the iOS target by
its shared scheme, on a macOS runner with an Xcode that carries the iOS 26 SDK
the Liquid Glass APIs need. It runs on changes to the crate, the runtime client, the iOS app, or
the lockfile, so a Swift break is caught in review rather than on a phone.

Not covered by those checks: a live provider turn against a real `glass-core`
binary, an actual APNs delivery (it needs a real key, topic, and device), and
steering on a real turn (the route and target matching are unit-tested, but the
local runtime never sees a provider that supports it). The simulator build
proves the Swift compiles; it does not exercise the app against a host. The
engine is restarted against the fixture, but a real `glass-core` crash is not
reproduced here.

## Design note: when one session is not enough

Every turn in this design runs in one `glass-core` session, because that is what
the host protocol offers today: one runtime process, one mixed-ownership
session, one turn at a time. That is the right shape for an assistant that is
mostly idle, and it is why a busy host answers `202 queued` instead of pretending
to run two things at once. It is the wrong shape for two people asking for
something at the same time, and it is worth naming the options before building
one.

*A small session pool.* Two or three runtime processes, each with its own
workspace and sessions directory, and a work queue that hands a turn to a free
one. Turns stop waiting behind each other, and a crash takes out one session
rather than the whole assistant. The cost is real: memory and provider
connections multiply, the "one conversation, one session" mapping has to become
explicit, and the control layer, cancel table, and presence document all have to
be keyed per session. Anything a person sees — approvals, presence, the ledger —
has to keep saying *which* session it is talking about.

*Human priority instead of parallelism.* Keep one session and make the queue
honest: typed messages jump ahead of routine firings, a person can preempt a
routine mid-turn by stopping it, and presence says why the wait is long. This is
much smaller, keeps one durable history, and covers the case that actually
happens on a personal host — one person, one assistant, several routines. That
is what this crate does: the turn queue holds a human lane and a routine lane,
the worker drains the human lane first, and presence reports both counts plus
the lane it takes next. The cost is that a long conversation starves scheduled
work instead of interleaving it, so a routine that came due at 09:00 can wait
until the conversation ends; the alternative hides that trade-off rather than
making it visible.

The pool is now built, deliberately small and keyed rather than free-floating.
Three lanes are enough for a personal host to work a question, a fix, and a
review at once without letting one process spawn engines without bound. The
mapping stays explicit: the conversation record names the lane that owns it, so
history, corrections, and later turns never land on an engine that has not heard
of the chat. Approvals, the cancel table, presence, and the ledger were already
keyed by conversation and task, so no surface has to guess which engine it is
talking about: the cancel registry holds every lane's running turns, presence
counts them together, and a lean poll reports each conversation's own running
turn. Human priority is unchanged inside a lane — typed messages still drain
before routine firings — so parallelism adds capacity without weakening the
queue's promise that a person's message runs before scheduled work.
