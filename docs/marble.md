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

### Presence

`GET /api/presence` answers what the assistant is doing: `working`, `waiting`
(an approval is pending), `paused`, or `idle`, plus uptime, the active task, the
queued count, the machine name, how many routines are scheduled, and how many
memories it holds. The same document is embedded in `GET /api/state` and
summarised in `/healthz`, so both surfaces and any script agree.

### Crash recovery

A restart must not leave a phantom turn behind. On startup, every task still
marked `running` is marked `interrupted`, a system transcript entry records what
happened, the routine that owned it is marked `interrupted`, and presence reports
`recoveredTasks`. Send the message again to continue.

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

Every path is resolved against one canonical workspace root. Absolute paths,
parent traversal, and symlinks that escape the root are refused before any
filesystem call. Search skips `.git`, `.ssh`, `.env*`, dependencies, and build
output, so a wide query cannot become a secret-read.

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
  them. `Resume queue` dispatches them in arrival order.
- `Stop everything` pauses admission and cancels every running task in one move.
  Both are reversible: the pause switch and the queue survive a restart.
- Routines are admitted through the same queue, so a paused assistant queues its
  scheduled work instead of running it.

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
| `/healthz` | GET | Liveness, revision, event sequence, posture, presence summary |
| `/api/login` | POST | Exchange the access token for an HTTP-only session cookie |
| `/api/state` | GET | Revisioned product document plus presence, memory, routines, voice, notifications, retention; `?lean=1` omits the transcript |
| `/api/transcript?conversationId=…&after=…` | GET | Bounded history for one conversation, resumable from a cursor |
| `/api/transcript/search?q=…` | GET | Newest transcript matches across conversations |
| `/api/presence` | GET | What the assistant is doing, uptime, recovery, counts |
| `/api/events?since=N` | GET | Server-sent events with bounded replay from a cursor |
| `/api/turns` | POST | Admit a durable turn, queue it, or apply an explicit memory command |
| `/api/turns/steer` | POST | Correct the running turn (`text`, optional `taskId` and `conversationId`) |
| `/api/tasks/{id}/cancel` | POST | Request cancellation of a running task |
| `/api/approvals/{id}/allow` and `/deny` | POST | Answer one pending governed tool; `{"remember":true}` also records a grant |
| `/api/control` | GET | Control document: posture, pause, grants, denied paths, ledger, queue |
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
| `/api/routines/{id}/run` | POST | Fire a routine now, queueing it when paused |
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

A turn posted while admission is paused is not rejected: it is queued and the
route answers `202` with `"queued": true` and its position. Approvals fail closed
when nobody answers within the configured wait.

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
- **Control** — posture, pause and stop, grants, denied paths, the queued
  messages, and the decision ledger.

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
proves the Swift compiles; it does not exercise the app against a host.
