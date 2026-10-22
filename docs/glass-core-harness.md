# Glass Core resident harness

**Status: Current source behavior.** Glass Dev uses the standalone `glass-core`
program for resident agents by default. Core owns provider configuration and
credentials, model and session state, and its native coding tools; Glass routes
only its own integration tools. This is separate from the
`glass-browser-core` Rust crate, which contains the native browser engine.
Glass starts `glass-core runtime`, sends JSON-RPC 2.0 messages over
stdin/stdout, with Core-native coding tools and Glass-owned integrations
available in one session.

```text
AgentRegistry
  └─ ResidentHarness (engine selection)
       └─ GlassCoreRuntime (Rust worker, bounded JSON-RPC 2.0 frames)
            └─ glass-core runtime (model loop, recovery, compaction)
                 ├─ Core coding tools ──→ Core execution policy
                 └─ glass.* calls ──→ Glass integration router
                                         └─ browser · Git · project authority
```

The Pi SDK runtime remains available only through the explicit legacy setting.
Changing the engine does not move project ownership: Core owns provider
configuration, credentials, model/session state, and its native coding tools;
Glass owns browser, Git, project, workspace, and collaboration integrations.

## Why the engine changed

Glass previously ran the resident model loop through Pi's `AgentSession` SDK in
a Node child process. That added a Node runtime beside the Rust workspace.
The default `glass-core` runtime removes Node from the resident path. It also
uses one session-record format and accepts Glass's tool catalog directly.

The engine owns exactly four things:

| Concern | Owner |
|---|---|
| Model loop, retries, recovery, compaction | `glass-core` |
| Session records and resume/fork/rewind | `glass-core` |
| Provider selection, credentials, model/session state | `glass-core` |
| Native coding tools and their execution policy | `glass-core` |
| Browser, Git, project/workspace integrations and their schemas | Glass |
| Integration approvals, evidence, workspace revisions and leases | Glass |

Glass negotiates runtime protocol v2 and requires the `mixedTools`
capability. The session uses `toolMode: "mixed"`: Core keeps its native
coding tools and Glass registers only its namespaced `glass.*` integrations.
Core executes its own coding tools; Glass registers browser and Git tools,
workspace/project inspection, read-only editor and language-server context,
non-writing editor proposals, and Glass project-task APIs. Editor writes,
generic commands, process controls, and Glass agent/model/session controls are
not exposed in the mixed catalog. Only registered Glass integrations cross
`tool/execute` and receive Glass-side authorization and evidence. A v1-only or older runtime
is reported as incompatible; Glass does not silently fall back to a
host-exclusive catalog. Core's standalone `local` mode and generic `host` mode
retain their existing behavior.

## Selecting the engine

| Setting | Effect |
|---|---|
| *(unset)* | Glass Core. The default for every resident agent. |
| `GLASS_AGENT_HARNESS=glass-core` | Same as unset; explicit. |
| `GLASS_AGENT_HARNESS=pi` | Legacy Pi SDK backend. |
| `GLASS_CORE_BINARY=/path/to/glass-core` | Use a specific engine executable. |

Glass does not install the engine when it starts. Without an explicit path, it
looks for `glass-core` on `PATH`, then `gc`, and reads its version. Run
`glass agent doctor` or `glass agent status` to see the selected engine,
protocol version, provider readiness, session directory, and repair steps.
These commands do not print credentials.

From the Glass TUI, `:agent setup login` resolves the same Core executable,
suspends the Glass screen, and starts `glass-core` in its interactive mode in
the current workspace with the terminal streams attached. Use Core's `/login`
there, then exit Core to return to Glass. Glass does not receive or store
provider credentials. After return, Glass refreshes Core readiness. The
resident Core process and its existing conversation are left running with
their current in-memory settings; a newly spawned resident agent starts a new
Core runtime and reads the updated configuration. `:agent new` reuses the
existing runtime and therefore does not reload provider settings.

The Agent palette labels the Core login and model controls explicitly. `:agent
models [AGENT_ID]` and `:agent model [AGENT_ID] PROVIDER MODEL` operate on the
selected resident session (or the workspace's first active session when none
is selected) using the negotiated `models/list` and `session/configure`
methods. Model catalog responses appear in the Agent conversation with the
provider, model IDs, and labels when Core supplies them. Without a selected or
active resident session, these controls report that a session must be started
first. With `GLASS_AGENT_HARNESS=pi`, the palette and
readiness panel instead describe the legacy Pi runtime and preserve its managed
setup and interactive login behavior.

The standalone source product is maintained in this repository at
[`products/glass-core`](../products/glass-core/README.md). To install its
current checkout for Glass, run
`products/glass-core/scripts/install.sh` from the monorepo root. The installer
provides `glass-core` and its `gc` alias. It remains separately installable;
its Rust packages share the root Cargo workspace version and lockfile and
retain their product license metadata. See the
[standalone product guide](glass-core-product.md) for scoped build and package
commands.

## Session lifecycle

1. `initialize` — protocol version `2`, required capability `mixedTools`, and
   an absolute session storage directory. Glass rejects an engine that does
   not advertise the negotiated contract.
2. `session/new` — the canonical workspace root, `toolMode: "mixed"`, only the
   Glass integration catalog, and the Glass collaboration brief. With `resume`, Glass lists
   stored sessions and resumes the newest mixed-mode session whose `cwd`
   matches the workspace root. Existing host-mode sessions remain available
   in Core but are not resumed under the changed ownership contract.
3. `turn/prompt`, `turn/steer`, `turn/followUp`, `turn/cancel` — conversation
   control. Steering interrupts the current request while preserving context;
   follow-ups queue behind the active turn.
4. `tool/execute` — Core asks Glass to run one `glass.*` integration call.
   Glass executes it against the live workspace revision and answers with
   bounded result text. Core-native coding tools execute inside Core.
5. `settled` — the turn ended. Glass maps it to `agent_end` and `agent_settled`,
   which returns the resident agent to idle.
6. `shutdown` — Glass cancels an active turn, asks the engine to close, and
   kills the process if it does not exit within the grace period.

Sessions are stored under the platform local data directory in
`glass/core-sessions` unless a caller supplies another absolute directory.

## Tool execution and approvals

Every Glass integration call becomes a Glass `ToolCall` and goes through the same
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
the runtime methods directly. Core provider credentials and configuration stay
in Core; Glass forwards no credential material.

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
