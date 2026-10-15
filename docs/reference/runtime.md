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
- `maxTokens`: output token limit, validated before the first request.
- `imageInput`: whether inline image inputs and tool results are accepted.
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
| `session/list` | Optional `afterSessionId`, `limit` | Page session IDs, names, lineage, and open state |
| `session/resume` | Optional expectedToolMode | Open the saved conversation |
| `session/close` | None | Release the writer; preserve files |
| `session/inspect` | Optional `cursor`, `limit` | Page transcript records and model settings |
| `session/configure` | Any optional setting above | Change configuration while idle |
| `session/fork` | Optional `throughTurn` | Create a closed branch through a user turn |
| `session/rewind` | `throughTurn` | Hide subsequent user turns from future context |
| `session/compact` | Optional `instructions` | Compact using the configured provider |

Resume accepts sessionId alone for host-owned sessions. GC loads the frozen
manifest and reports its configuration through session/describe. expectedToolMode
defaults to host; resuming a local-execution session requires an explicit local
expectation. Any creation fields supplied with resume must match the manifest.
Tool mode and schemas remain immutable. Provider, model, instructions, imageInput,
and maxTokens can change through session/configure while idle.

Effective transport/model settings are saved when the session is created.
Credential values are never stored in the manifest; saved connection IDs and
environment variable names are retained. The host owns its credential lifecycle.
Selecting a connection applies that account's defaults before explicit overrides.

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
{"jsonrpc":"2.0","id":"host:SESSION:1234:call-1","method":"tool/execute","params":{"sessionId":"SESSION","turnId":"turn-1","callId":"call-1","name":"read_file","arguments":{"path":"README.md"}}}
```

The host applies policy, executes the exact call, and responds:

```json
{"jsonrpc":"2.0","id":"host:SESSION:1234:call-1","result":{"text":"File contents","isError":false}}
```

Return isError=true for denied approval, validation failure, or execution error.
The isError field is optional and defaults to false; when supplied it must be a
boolean.
Errors are recorded and returned to the model so it can recover. Text is bounded
to 512 KiB. Unknown, duplicate, or late tool responses cannot satisfy a later
request. Host calls execute sequentially; a host may implement its own parallel
operation behind one tool.

On tool_cancel, stop the matching host operation if possible and acknowledge
its actual execution status. Without acknowledgement, GC records unknown effects.
The host must reconcile those effects before authorizing further mutations.
GC does not automatically replay host mutations after disconnect.

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

Runtime v1 supports an isolated prefix/suffix completion lane and inline image
inputs. These are protocol capabilities, not guarantees that a particular model
supports vision or enforces reasoning hints. ACP remains a separate adapter with
its existing capability response.

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

## Structured sessions and incremental reads

| Method | Parameters | Result |
| --- | --- | --- |
| session/entries | sessionId, optional cursor and limit | Structured records, stable entry IDs, next byte cursor |
| session/entry | sessionId, entryId, field, offset, limitBytes | A UTF-8 chunk of a large string field |
| session/name | sessionId, name | Persisted display name |
| session/stats | sessionId | Turn/tool counts, journal size, reported usage |
| session/tree | optional afterSessionId and limit | Paged session nodes with parent edges |
| session/describe | sessionId | Frozen manifest and execution ownership |
| session/validate | sessionId | Credential/configuration validation and configured limits |

Entry IDs use journal byte positions and remain stable across append-only
compaction, close, and resume. They are opaque identifiers to clients. Entries
retain structured durable record fields: k identifies user, assistant, call,
tool, image, custom, or other GC records. Tool calls retain id/name/args; results
retain id/name/t. args contains the original JSON argument string.

Read session/entries with cursor=0, then retain its returned cursor for later
polls. GC seeks directly to that byte position. A page contains at most 100
records and 256 KiB of previews. String fields are previewed at 4 KiB; image
base64 is omitted. truncatedFields identifies fields that need session/entry.
That method returns up to 64 KiB per chunk. Cursors must be record boundaries.
Repair of an incomplete crash tail may invalidate a cursor beyond the repaired
end; reload from zero if that happens.

Session lists and trees contain at most 128 nodes per page and use
nextAfterSessionId. Parent edges describe conversation forks; they do not imply
workspace rollback. Statistics are cached until the journal grows. reportedUsage
counts received runtime turn usage; absent usage is not guessed, completion
usage is separate, and usageComplete remains false.

## Typed context, images, and results

turn/prompt, turn/steer, and turn/followUp accept context and images alongside
text. Context is an array of typed data blocks:

```json
{"context":[{"type":"text","text":"Selected source"},{"type":"resource","uri":"file:///project/a.rs","text":"Inline contents"},{"type":"json","value":{"selection":{"line":12}}}]}
```

GC does not fetch resource URIs. Context is recorded as data and projected at
user-message priority, not promoted to system instructions. It is bounded to
64 KiB per request. A correction's context and images reach the model with that
correction, after pending tool results have been resolved.

images is an array of mediaType/data objects. data is base64. PNG, JPEG, WebP,
and GIF signatures and dimensions are validated. Limits are four images per
request, 512 KiB encoded per image, 32 million pixels, and the overall 1 MiB
transport frame. imageInput=false rejects image inputs and results before use.
No image preparation process or file fetch runs implicitly.

Host results may include content blocks of type text, json, or image, plus
structuredContent. JSON blocks use value; image blocks use mediaType and data.
Text-only result objects remain supported. Image bytes are recorded separately
and sent after all results in a tool batch, preserving provider call/result
ordering. They are not copied into textual model output.

maxTokens accepts 1..128000. reasoningBudget cannot exceed maxTokens; Anthropic
positive budgets must be at least 1024 and below maxTokens. Other transports use
model-specific hints. Validation reports unknown when GC has no supported
reasoning mapping; it does not promise that a provider honors a hint.

## Tool progress, cancellation, and reconciliation

tool/progress is a host notification containing sessionId, requestId, and text.
GC forwards tool_progress events without storing them or sending them to the
model. Updates are bounded to 64 KiB each, 512 KiB per tool, and 1024 updates per tool.
Empty notifications are ignored. Overflow produces one truncation marker. The final result
remains the source for model context and durable output.

A tool_cancel event asks the host to stop the exact operation. Reply with a
tool/cancelled request containing sessionId, requestId, and executionStatus:
cancelled, completed, or unknown. The host may instead deliver the original
completed tool result. GC waits cancelGraceMs (default 250, configurable 0..5000
during initialization), emits tool_cancel_ack when acknowledged, and records
tool_status. An acknowledgement of completed without a result says effects
occurred but output is unavailable; it does not authorize replay.

Without acknowledgement, effects remain unknown. session/interruptedTools
returns up to 128 unresolved operations and a truncation flag. Older operations
remain in the journal and can be reconciled directly by requestId. Use
tool/reconcile while idle with requestId, executionStatus, and an optional
bounded text summary. GC records the host's finding and supplies it as context
data on later turns. Reconciliation records are assertions by the host, not
independent verification by GC.

Execution request IDs include a durable journal coordinate, so a restarted host
reusing its own request counters cannot collide with earlier operations.
Request IDs are opaque. Tool call IDs still preserve provider identifiers.

## Discovery and completion

auth/status reports credential readiness and refresh requirements without a
model request. It does not establish provider reachability. models/list performs
explicit catalog discovery and caches up to four catalogs for 60 seconds.
Use refresh=true to bypass the cache, cursor/limit to page results. Catalog
responses are bounded to 2 MiB and 2048 retained models. Unverified vision and
tool support remain null; reasoning mappings are labeled as hints.

completion/request accepts sessionId, prefix, suffix, and optional maxTokens
(default 1024, maximum 4096). Prefix plus suffix is bounded to 128 KiB. GC sends
a fresh tool-free request with no conversation, instructions, skills, or memory.
Returned text is the model output, including whitespace. Hosts should validate
an insertion before applying it; model compliance is not guaranteed.
It streams completion_text and ends with completion_settled, returning text,
stopReason, and any reported usage. Reasoning reduction is a provider-dependent
hint. Completion does not append conversation records; billing still uses the
session ledger. Hosts remain responsible for validating a proposed insertion.

completion/cancel targets the pending request with sessionId and turnId.
Completion output is bounded to 64 KiB. No tool call from this lane is executed.
Reasoning-only, malformed, or incomplete provider output fails explicitly.
The output is an instruction-based insertion, not a claim that every endpoint
offers a native FIM API.

RPC errors include data.kind and retryable. They distinguish protocol, unsupported
method, invalid input, busy, limit, authentication, provider, and operation
failures. retryable is a hint, not permission to replay a mutation. Settlement
and tool execution status must also be checked.

## Host clients and acceptance

The [TypeScript client](../../packages/runtime-client/README.md) supplies typed
requests, events, results, controls, and bounded asynchronous writes. The
[Rust client](../../crates/glass-runtime-client/README.md) provides typed protocol
values and event polling. Both have actual subprocess acceptance tests.

The external conformance suite imports only the reference client and drives a
release executable through public messages:

```sh
GLASS_CORE_TEST_BINARY="$PWD/target/release/glass-core"   python3 scripts/conformance-runtime.py
```

The live trial uses the machine's selected GC connection and changes only a
temporary fixture project. It requires explicit opt-in:

```sh
python3 scripts/host-kilo-trial.py --live --binary target/release/glass-core
```

It exercises a real coding task, host-owned tools and approvals, a custom user
answer, progress, structured context/results, tests, session resume, steering,
cancellation, and completion. Model-specific vision behavior requires a
vision-capable provider trial; offline conformance verifies the image protocol.
