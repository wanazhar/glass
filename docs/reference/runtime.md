# Host runtime protocol

[Documentation index](../INDEX.md) · [Automation](../automation.md)

`gc runtime` exposes an opt-in JSON-RPC 2.0 subprocess interface. It runs the
existing agent engine without the terminal UI. Hosts can be editors, agent
development environments, desktop applications, or automation services.

The standalone `gc`, `gc run`, ACP, and served-session interfaces retain their
existing behavior. Runtime v1 is a separate protocol; it does not claim Pi SDK
compatibility or replace ACP.

## Ownership

| Concern | Host tool mode | Local tool mode |
| --- | --- | --- |
| Model loop, recovery, compaction | GC | GC |
| Tool schemas and execution | Host only | GC built-ins |
| Tool permissions and approvals | Host | GC configured policy; no interactive terminal |
| Questions | Host-provided question tool | Not advertised |
| Output and interaction UI | Host | Host |
| Credentials | GC configuration/environment | GC configuration/environment |
| Session location | Explicit initialization or isolated runtime directory | Same |

Host mode is the default. It exposes no built-in filesystem, shell, state,
memory, browser, or extension tools. The host must register every tool it wants
the model to use. A tool named `read` or `bash` is still a host tool: GC does
not substitute its own implementation.

The host owns all effects of delegated tools. Before executing a mutation, the
host can display an approval prompt and wait for the user. Denial returns an
error tool result. GC waits without making another provider request. A
question tool works the same way and may accept free-text answers. The runtime
does not infer tool permissions from names.

## Transport and lifecycle

Start the executable directly with argument `runtime`. Use stdin and stdout
pipes; keep stderr separate for diagnostics. Each message is one UTF-8 JSON
object followed by a newline. Requests require a unique nonempty string ID of at most 128 bytes, or a number ID.
Responses echo that ID. GC-generated tool requests use string IDs.

Initialize before any session operation:

```json
{"jsonrpc":"2.0","id":"init","method":"initialize","params":{"protocolVersion":1,"storageDir":"/absolute/private/session-directory"}}
```

The response reports protocol version, runtime version, supported capabilities,
limits, and the resolved storage directory. Unsupported protocol versions fail.
Without `storageDir`, GC uses its data directory's `runtime` subdirectory.

One subprocess runs one turn at a time and retains up to 32 open conversations.
Use separate processes for concurrent agents. Sessions take exclusive writer
locks. `session/close` releases a conversation without deleting it.
`shutdown` closes the idle runtime. During a turn, cancel it and wait for
settlement before shutdown.

EOF cancels a running turn or pending host-tool wait. GC preserves partial
conversation records. The host must independently cancel and reconcile tools
it executes; killing GC cannot undo an external mutation.

Both directions enforce 1 MiB frames. The input queue holds 64 frames and the
follow-up queue holds 32 requests. Oversized input closes the connection after
an error. Unknown/busy requests return errors. Stdout writes apply backpressure;
the host must continuously drain output. This is a trusted local process
protocol, not an authenticated network service or an operating-system sandbox.

## Sessions

Create an isolated host-controlled conversation:

```json
{"jsonrpc":"2.0","id":"new","method":"session/new","params":{"cwd":"/absolute/workspace","toolMode":"host","instructions":"Help the user using the supplied tools.","tools":[{"name":"read_file","description":"Read a workspace file through the host.","inputSchema":{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}}]}}
```

The response contains `sessionId`. The workspace must be an existing absolute
directory. Tools use unique ASCII alphanumeric, underscore, or hyphen names.
Schemas must describe JSON objects. GC validates the structural JSON Schema
subset used by its existing tool-repair mechanism; the host must validate any
additional constraints before execution.

Optional session settings:

- `model`: model ID within the selected provider configuration.
- `provider`, `baseUrl`: explicit transport configuration.
- `connection`: saved GC account ID, or null.
- `apiKeyEnv`: environment variable name, or null; never send a credential value.
- `reasoningBudget`: unsigned token budget; zero requests reduced reasoning
  where supported.
- `instructions`: host-authored system instructions. Host mode does not load
  project instructions, skills, neuralnets guidance, or extensions implicitly.
  Supply required project guidance yourself.

Local mode uses `"toolMode":"local"` and omits `tools`. It uses GC built-ins
under the configured execution policy. Runtime local mode does not load
configured extensions or offer interactive permission prompts. Use host mode
when the application needs approvals or questions.

| Method | Parameters beyond sessionId | Behavior |
| --- | --- | --- |
| `session/list` | None | List runtime session IDs and open state |
| `session/resume` | Original creation parameters | Open the saved conversation |
| `session/close` | None | Release the writer; preserve files |
| `session/inspect` | Optional `cursor`, `limit` | Page transcript records and model settings |
| `session/configure` | Any optional setting above | Change configuration while idle |
| `session/fork` | Optional `throughTurn` | Create a closed branch through a user turn |
| `session/rewind` | `throughTurn` | Hide subsequent user turns from future context |
| `session/compact` | Optional `instructions` | Compact using the configured provider |

Resume requires the original parameters plus sessionId, including tool schemas.
A mismatch fails rather than silently changing execution ownership. If session
configuration changed, use the updated parameters. Tool mode and tool schemas
cannot change within a conversation; create a new session for a different tool
contract. Provider configuration inherited from the machine may change between
processes; pass explicit settings when reproducibility is required.

Inspection returns record count, bounded text previews, truncation flags, and
`nextCursor`. It is a presentation interface, not a lossless session export.
Use the stored GC JSONL conversation for full records. Fork defaults to the
latest user turn and returns a sessionId that can be resumed. Rewind changes
model context only; it never restores workspace files or erases audit records.

Session manifests and logs are private files. They contain tool schemas,
instructions, provider endpoint configuration, user text, and tool results.
Choose a private storage directory and follow normal GC retention practices.

## Turns and controls

```json
{"jsonrpc":"2.0","id":"turn-1","method":"turn/prompt","params":{"sessionId":"SESSION","text":"Review the project"}}
```

The prompt request completes after the turn settles. Its ID is also the
`turnId` on every event and tool request for that turn.

While it runs:

- `turn/steer`: sessionId, optional turnId, and text. Interrupts the current
  request/tool wait, preserves context, and continues the same turn with the
  correction before the next provider request or tool.
- `turn/followUp`: sessionId and text. Queues a new turn after the current one.
  Its response arrives when that follow-up finishes.
- `turn/cancel`: sessionId and optional turnId. Requests cancellation.
  Wait for `settled`; acceptance alone does not establish tool cancellation.

One correction or cancellation may be pending at a time. Targeting a different
active turn fails. Idle cancellation is a harmless response with accepted=false.
Other controls, including session mutation, fail busy while a turn runs.

## Events

Notifications use `runtime/event`:

```json
{"jsonrpc":"2.0","method":"runtime/event","params":{"sessionId":"SESSION","turnId":"turn-1","type":"text","data":{"text":"Reviewing the file."}}}
```

Event types include `turn_start`, `waiting`, `text`, `reasoning`,
`tool_start`, `tool_end`, `tool_cancel`, `usage`, `compacted`, `steered`,
`queued`, `notice`, `error`, and `settled`. Reasoning is forwarded only when
the provider exposes it. Tool events retain the provider's call ID. Tool-end
events contain text and isError. Text exceeding 64 KiB is split into events with
byte-based chunkOffset and chunkFinal fields; concatenate chunks for one call
before treating its final tool-end event as complete. Usage separates input, cache reads, cache
writes, and output; missing provider usage is not fabricated.

Preflight failures, such as invalid provider configuration, return a request
error before turn_start. No turn is running and no settled event is emitted for
that rejected request. Hosts must handle request errors as well as events.

Settlement reasons are completed, cancelled, or failed. A failed turn also
returns a JSON-RPC error. Completed means the model finished its turn, not that
the host's acceptance tests passed. Hosts should render a single final error
surface even when both an error event and failed settlement are received.

## Host tool requests

```json
{"jsonrpc":"2.0","id":"host:SESSION:turn-1:call-1","method":"tool/execute","params":{"sessionId":"SESSION","turnId":"turn-1","callId":"call-1","name":"read_file","arguments":{"path":"README.md"}}}
```

The host applies policy, executes the exact call, and responds:

```json
{"jsonrpc":"2.0","id":"host:SESSION:turn-1:call-1","result":{"text":"File contents","isError":false}}
```

Return isError=true for denied approval, validation failure, or execution error.
The isError field is optional and defaults to false; when supplied it must be a
boolean.
Errors are recorded and returned to the model so it can recover. Text is bounded
to 512 KiB. Unknown, duplicate, or late tool responses cannot satisfy a later
request. Host calls execute sequentially; a host may implement its own parallel
operation behind one tool.

On `tool_cancel`, stop the matching host operation if possible. Its outcome may
be uncertain; reconcile effects before further mutations. GC never retries a
host mutation merely because the host disconnected.

## Clients and first integration

[glass-runtime-client](../../crates/glass-runtime-client/src/lib.rs) provides a
Rust subprocess client with bounded input/output, asynchronous request IDs,
message polling, and tool results. It does not depend on GC's UI or engine.

[The reference Python host](../../examples/runtime_host.py) uses only the
standard library and demonstrates bounded file reads and a custom-answer
question tool:

```sh
python3 examples/runtime_host.py --binary gc --storage /tmp/gc-host-demo \
  --workspace "$PWD" "Read README.md and ask which part to improve"
```

Glass Dev is the intended first production integration. Its governed tool router
can implement tool/execute and its existing approval UI can pause that response.
This protocol and client do not switch Glass Dev away from Pi. The resident
Glass adapter and its acceptance tests remain separate integration work.

Runtime v1 advertises no FIM completion or image prompts. ACP remains a separate
adapter with its existing capability response. Additional adapters should reuse
the engine and negotiate capabilities rather than pretend unsupported controls
exist.

## Verification

```sh
cargo test --workspace --locked
cargo build --release --locked -p glass-core --bin glass-core
GLASS_CORE_TEST_BINARY="$PWD/target/release/glass-core" \
  python3 scripts/smoke-runtime.py
```

The offline fixture drives the actual executable against a local mock provider.
It covers exclusive tool schemas, real call IDs, a custom question answer,
fork/rewind/resume, steering, cancellation, and shutdown. Provider behavior and
Glass's authority boundary require their own integration acceptance tests.
