# Glass Core resident harness

**Status: Current source behavior.** Resident agents in Glass Dev run on the
`glass-core` engine. Glass starts `glass-core runtime` as a subprocess, speaks
its documented JSON-RPC 2.0 host protocol over stdin/stdout, and executes every
tool call itself.

```text
AgentRegistry
  └─ ResidentHarness (engine selection)
       └─ GlassCoreRuntime (Rust worker, bounded JSON-RPC 2.0 frames)
            └─ glass-core runtime (model loop, recovery, compaction)
                 └─ tool/execute ──→ Glass governed tool router
                                       └─ workspace · browser · process authority
```

The Pi SDK runtime is still present as a legacy backend. Nothing about Glass's
authority model changes between engines: Glass owns the workspace, the tool
schemas, the approvals, and the evidence in both cases.

## Why the engine changed

Glass previously drove Pi's `AgentSession` SDK through a Glass-owned Node child.
That kept two runtimes in one product: a Node toolchain for the model loop and
the Rust workspace for everything else. Running the model loop on `glass-core`
removes the Node dependency from the resident path, keeps session records in one
format, and lets Glass register its tool catalog directly instead of describing
it to a foreign extension host.

The engine owns exactly four things:

| Concern | Owner |
|---|---|
| Model loop, retries, recovery, compaction | `glass-core` |
| Session records and resume/fork/rewind | `glass-core` |
| Tool schemas, execution, approvals, evidence | Glass |
| Workspace revisions, leases, browser authority | Glass |

`glass-core runtime` is started in host tool mode, which exposes no built-in
filesystem, shell, state, memory, browser, or extension tools. The complete tool
catalog in the session is the frozen list Glass registered, so there is no path
for the model to bypass the governed router.

## Selecting the engine

| Setting | Effect |
|---|---|
| *(unset)* | Glass Core. The default for every resident agent. |
| `GLASS_AGENT_HARNESS=glass-core` | Same as unset; explicit. |
| `GLASS_AGENT_HARNESS=pi` | Legacy Pi SDK backend. |
| `GLASS_CORE_BINARY=/path/to/glass-core` | Use a specific engine executable. |

Without an explicit path, Glass resolves `glass-core` on `PATH`, then `gc`, and
reports the result of `glass-core --version`. `glass agent doctor` and
`glass agent status` print the selected engine, its protocol version, provider
readiness, session storage directory, and ordered remediation. They never print
credential values.

## Session lifecycle

1. `initialize` — protocol version `1` and an absolute session storage
   directory. Glass rejects an engine that does not advertise host tool
   ownership, and an engine older than the supported runtime surface.
2. `session/new` — the canonical workspace root, `toolMode: "host"`, the Glass
   tool catalog, and the Glass collaboration brief. With `resume`, Glass lists
   stored sessions, describes them, and resumes the newest one whose `cwd`
   matches the workspace root.
3. `turn/prompt`, `turn/steer`, `turn/followUp`, `turn/cancel` — conversation
   control. Steering interrupts the current request while preserving context;
   follow-ups queue behind the active turn.
4. `tool/execute` — the engine asks Glass to run one call. Glass executes it
   against the live workspace revision and answers with bounded result text.
5. `settled` — the turn ended. Glass maps it to `agent_end` and `agent_settled`,
   which returns the resident agent to idle.
6. `shutdown` — Glass cancels an active turn, asks the engine to close, and
   kills the process if it does not exit within the grace period.

Sessions are stored under the platform local data directory in
`glass/core-sessions` unless a caller supplies another absolute directory.

## Tool execution and approvals

Every host tool call becomes a Glass `ToolCall` and goes through the same
governed router the TUI uses. A call that mutates the workspace follows the
session's authority:

- An unrestricted human-controlled session runs the call and records evidence.
- Otherwise Glass parks the call, emits a `glass_tool_approval_request`
  conversation event, and answers the engine only after the human approves or
  denies that exact serialized call. Approving once authorizes one call.

Results are bounded to 512 KiB, and denial returns `isError: true` with the
reason, so the model can recover instead of stalling. Glass records a
`glass_tool_evidence` or `glass_browser_evidence` event for every executed
call, with secret-like tokens redacted.

## Engine gaps Glass reports instead of hiding

The runtime surfaces differ in a few places. Glass reports the gap as a request
error rather than silently ignoring it:

| Request | Glass Core behavior |
|---|---|
| `SetThinking` | Reasoning budget comes from the session model; use `session/configure` or the engine's model selection. |
| `Rewind`/`Fork` by entry id | Glass Core identifies turns by number, not journal entry id. |
| Slash commands | Runtime sessions expose no slash-command surface; use Glass actions. |

Everything else — prompt, steer, follow-up, abort, models, sessions, entries,
tree, stats, naming, compact, fork, resume, and isolated completion — maps onto
the runtime v1 methods directly.

## Verification

The adapter is covered by unit tests for request mapping, event translation,
evidence redaction, and tool registration, plus fixture tests that drive a fake
`glass-core runtime` over the real protocol: session startup, resuming the
newest stored session for the workspace, a full turn with a host tool call, a
parked approval that resumes on the human decision, and rejection of an unknown
approval frame.

```sh
cargo test -p glass-dev --lib core_runtime
cargo test -p glass-dev --lib resident_harness
```

The fixture runs offline and needs no provider account:

```sh
python3 crates/glass-dev/tests/fixtures/fake_glass_core_runtime.py runtime
```

End-to-end verification against the real engine additionally needs a built
`glass-core` executable:

```sh
GLASS_CORE_BINARY=/path/to/glass-core glass agent doctor
```
