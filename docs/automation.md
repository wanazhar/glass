# Headless execution and remote sessions

[Documentation index](INDEX.md) · [Project overview](../README.md)

## Headless execution

```sh
glass-core run "fix the failing build"
glass-core run --json --budget 2 "review this change"
glass-core run --json --resume SESSION_ID "continue the task"
glass-core run --json --resume SESSION_ID --retry --model MODEL_ID
glass-core run --json --attach src/main.rs "review this file"
glass-core sessions --json
glass-core context SESSION_ID --json
glass-core review SESSION_ID --json
printf '%s' 'explain this repository' | glass-core run --json
```

Headless execution uses the same tools, extensions, project instructions,
compaction, and task checkpoints as the terminal. It saves a resumable session.
`--json` emits one JSON object on stdout with session ID, final text, status,
error, cost, and checkpoint error; diagnostics go to stderr. Exit codes are
0 for a finished agent turn, 1 for an error, 2 for an exhausted tool-round limit,
and 3 for reaching the session cost limit. SIGINT/SIGTERM stop owned shell
process groups, save partial responses, emit `status: "cancelled"` in JSON,
and exit with 130/143. Parse and preflight failures also emit a JSON error
when `--json` is requested. `--connection ID` chooses a saved account without
changing the global preference; use `glass-core status --json` to find IDs.
Run interactively over SSH with `ssh -t host gc`; non-interactive runs require
no terminal. Browser sign-in over SSH supports pasting the final loopback
callback URL in `/login`; API keys can also be configured through environment
variables. There is no credential import from another harness. A finished turn is the model's
completion signal; callers should still check their own build or test results.

## Host-controlled runtime

Use `gc runtime` when your application owns tool execution, permissions, questions,
and presentation. The [runtime protocol](reference/runtime.md) defines versioned
JSON-RPC messages, exclusive host tools, sessions, steering, and streamed events.
A reusable Rust client and a standard-library Python reference host are included.

## Editor integration (ACP)

`gc acp` serves Agent Client Protocol v1 over stdio. Supported editors can create
sessions and submit prompts. Responses stream as message chunks, provider-exposed
reasoning chunks, tool-call updates, usage updates, and a stop reason.

The adapter uses the same Agent runtime, tools, context management, cost limits,
trust handling, and configured execution policy. Cancellation uses the host's
cancellation path and stops owned shell groups.

The current interface does not advertise session loading, mode switching, or
client-side filesystem/terminal callbacks. Host tools execute locally. Configure
MCP servers on the host through the bridge; client-provided MCP configuration is
not activated. Consult the current ACP capability response before using optional
protocol methods.

## Served sessions

A served session continues after a client detaches:

```sh
gc serve
gc attach --socket SOCKET_PATH
gc logs SESSION_ID
gc stop SESSION_ID
gc respawn SESSION_ID
```

The server owns the session loop, tools, extensions, checkpoints, ledger, trust,
and configured confinement. It uses a private Unix socket in the data directory.
`attach` can discover the newest socket, replay retained entries, submit prompts,
and cancel work. Up to four attached clients receive live broadcasts. A client
that disconnects does not stop the owner or block other clients.

Access is controlled through local socket permissions. This session server does
not expose a TCP listener. Optional companion interfaces have their own access
contract. Avoid treating a socket pathname as proof of server liveness.

`stop` requests shutdown after active work reaches its supported shutdown point.
`respawn` starts a new owner for the same session in a separate process group.
`logs` reads the durable log without opening it for writing or consuming recovery
state. Omitted session IDs use the supported newest-session selection.

Reconnect uses bounded backoff, replay positions, and durable submission IDs.
Ctrl+C or Ctrl+D can cancel reconnecting. Slash modals remain local to the
interactive application.

## Messaging another session

Save a follow-up to another session's inbox:

```sh
gc send SESSION_ID "The migration finished; review the updated files"
gc send --latest "Continue the saved task"
```

The interactive form is `/send SESSION_ID MESSAGE`. Messages are appended to a
private inbox and attributed to their source. A running owner reads its inbox
while idle; accepting a message does not establish provider execution or completion.
Messages are capped at 32 KiB. Target IDs are validated, and partial concurrent
appends are not treated as complete messages.

## Loops

A loop submits a saved prompt at an interval while its session is idle:

```text
/loop 30m check the build status and report changes
/loop
/loop rm 1
/loop clear
```

Intervals accept seconds, minutes, or hours; bare numbers mean seconds. Zero is
rejected. A session supports up to 16 loops. A busy turn or an active draft delays
delivery. Missed intervals advance instead of accumulating an unbounded queue.

Schedules persist beside the session and resume with it. A served owner can keep
running schedules after the terminal detaches. Dispatch receipts record submitted
work; they do not prove completion. Loop-triggered turns consume provider quota
and use the same tool permissions and cost accounting as other turns.
