# Native Pi SDK runtime

**Status: Current 0.3.13 source behavior.** The resident Glass agent uses Pi's native `AgentSession` SDK. Glass does not start the Pi CLI or use the Pi compatibility protocol for this resident path. A Glass-owned Node child loads the SDK and exchanges private 4-byte big-endian length-prefixed JSON frames with Rust.

```text
AgentRegistry
  └─ GlassPiRuntime (Rust worker and bounded IPC)
       └─ pi-runtime.mjs (Glass-owned Node child)
            └─ @earendil-works/pi-coding-agent AgentSession
                 └─ Glass tools → resident workspace actor/router
```

This guide covers the native resident path. The one-shot CLI option
`glass agent prompt --harness pi` still uses the compatibility Pi command adapter;
it is described under [CLI and adapter distinction](#cli-and-adapter-distinction).

## Readiness, setup, and version status

Check readiness before creating a resident agent:

```console
glass agent doctor
glass agent status
```

Readiness reports Node, SDK, authentication, provider metadata, agent directory, managed runtime root, newest session, and remediation. It never prints secret values. Node must be `22.19.0` or newer. Setup does not run during Glass startup and does not download anything implicitly.

```console
glass agent setup
glass agent setup --update
glass agent setup --login
glass agent setup --sdk-entry /path/to/dist/index.js --agent-dir /path/to/pi/agent
```

Without `--sdk-entry`, setup installs the exact Rust-managed pin under the platform local-data directory `glass/runtime/pi`, writes `selected-runtime.json`, and uses its `agent` directory. `--update` reinstalls that exact pin. It is not an automatic upgrade to an unreviewed upstream version. `--sdk-entry PATH` selects an existing `dist/index.js`; `--agent-dir PATH` is valid with `--sdk-entry` and selects the credential/config directory.

There are two checked-in version facts:

| Evidence | Version and status |
|---|---|
| Published `0.3.12` release evidence | Pi SDK `0.84.2`; see [release notes](releases/0.3.12.md) |
| Current `0.3.13` source and package metadata | `PINNED_PI_SDK_VERSION`, managed setup, and the checked-in `packages/pi-runtime` package/lockfile use `0.84.3` |

The current `0.3.13` source and package metadata are aligned on `0.84.3`.
Do not describe `0.84.3` as part of the immutable `0.3.12` release; keep the
published `0.3.12` SDK fact scoped to its historical release record. Treat
the `0.3.13` details as current-source/release-record information until
publication evidence is available.

`setup --login` opens the selected Pi CLI with `PI_CODING_AGENT_DIR` set to the selected agent directory. Run Pi `/login`, then exit Pi. Glass does not impersonate a provider or copy credentials. `--login` requires the Pi CLI to be available either in the managed installation or on `PATH`; a non-zero Pi exit is an explicit setup error.

Readiness accepts one of the supported environment API keys (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, `GOOGLE_API_KEY`, `MISTRAL_API_KEY`, `GROQ_API_KEY`, `XAI_API_KEY`, `OPENROUTER_API_KEY`, `ZAI_API_KEY`, or `AWS_BEARER_TOKEN_BEDROCK`) or a Pi `auth.json` with at least one provider. Empty provider metadata is `missing`; unreadable or invalid JSON is `unknown`; when all stored credentials have expired timestamps, state is `expired`. Project and browser features remain usable without Pi authentication.

## Agent and session lifecycle

`AgentRegistry` owns up to 32 resident agents. Each agent has one worker thread, one GlassPiRuntime child, one persistent session directory, one role, optional model and thinking level, optional worktree, and bounded evidence. A dependency-ready agent starts in `starting`, emits `ready`, becomes `idle`, and accepts `prompt`, `steer`, `follow-up`, control, and inspection requests. Prompt or follow-up turns remain `working` until the native session emits `agent_end` and the adapter emits `agent_settled`. The registry then returns the agent to `idle`.

```text
create → starting → idle
                     │
       prompt/follow-up/steer
                     ▼
                  working ── agent_settled ──► idle
                     │                         │
             abort/failure                    controls
                     ▼                         │
             cancelled/failed ◄── restart ────┘
```

A worker failure, malformed frame, closed event stream, or budget breach becomes `failed`. `cancel` aborts and shuts down a non-terminal worker. `restart` is valid only for `failed` or `cancelled` agents and keeps the agent ID; completed agents remain terminal. Registry drop sends shutdown and joins owned workers. Dependent agents fail closed when a required agent fails or is cancelled.

The daemon-backed resident workspace retains the registry while the workspace actor is alive. Client disconnect does not stop it. Workspace close, daemon shutdown, idle project eviction, or process failure ends ownership. Session files can be selected or resumed explicitly, but a session file does not prove that its old worker or PTY survived a crash.

## Protocol and SDK surface

The adapter sends `hello`, `prompt`, `steer`, `followUp`, `abort`, `state`, `models`, `setModel`, `setThinking`, `newSession`, `compact`, `cloneSession`, `rewind`, `fork`, `switchSession`, `listSessions`, `entries`, `tree`, `messages`, `stats`, and `setName` operations. Events are forwarded to the worker as structured Pi events. `message_update` token noise is not required for the TUI display; the display consumes bounded evidence and settled state.

The adapter starts `createAgentSession` with `DefaultResourceLoader` and disables Pi built-in tools, extensions, skills, prompt templates, themes, and context files. The allowed native tools are Glass `glass_tool`, `delegate`, `read`, `write`, `edit`, `bash`, `grep`, `find`, and `ls`. `glass_tool` accepts an exact `glass.*` capability name and JSON arguments. Familiar file and shell names are normalized to Glass names before execution.

All tool execution returns through the authoritative workspace router. The router checks trust, actor attribution, expected generation and project revision, path confinement, mutation authority, browser policy and leases, and result bounds. Browser tools are unavailable without an attached healthy Browser Workspace. A browser result includes bounded evidence such as redacted URL, title, target, workflow state, and revision; query strings and fragments are removed.

## Tool, approval, and queue limits

| Boundary | Limit or behavior |
|---|---|
| Rust ↔ Node frame | 16 MiB maximum; zero-length and oversized frames fail |
| Rust output event channel | 256 frames; worker event delivery can drop lossy Pi events and reports dropped count |
| Agent registry | 32 agents |
| Agent history | 512 retained events; per-agent evidence 64 values |
| Agent prompt/steer/follow-up | 64 KiB; attached context must be a JSON object ≤64 KiB |
| Native Pi pending approvals | One Glass mutation approval at a time; a second tool call while pending is a conflict |
| Unknown tool recovery | Return the tool error for the first two failures; abort the turn on the third unknown-tool failure |
| Tool result | Shared router result limit 512 KiB; core agent gateway result limit 64 KiB |
| `bash` timeout | 1–300 seconds in the native tool schema |
| `delegate` timeout | 1–3,600 seconds in the native tool schema |
| Session path | Must resolve inside the explicit session directory |

A native mutation tool call produces `glass_tool_approval_request` with redacted arguments. The resident host must resolve the matching frame with approve or deny. Approval is consumed once and applies only to that exact call. A denial returns an error to Pi. There is no source-level 120-second approval expiry in the native adapter; do not promise one. Stale, unknown, duplicate, or concurrent approval frames fail closed. The one-shot compatibility adapter has no interactive approval host and denies Pi extension UI requests immediately.

`glass --yolo` is an explicit unrestricted mode for the Glass process. It skips Glass mutation confirmation, accepts Pi extension confirmation RPCs, enables ambient Pi resources and extension tools, and allows configured browser capabilities. It does not remove stale revision checks, path checks, leases, protocol bounds, timeouts, or result limits. Treat installed extensions, shell commands, and project instructions as local code under the operating-system account.

## Persistent sessions and migration failures

Resident sessions default to the workspace `.glass/pi-sessions` directory. The native adapter uses `SessionManager.create(cwd, sessionDir)` for a new session and `SessionManager.continueRecent(cwd, sessionDir)` only when its worker is explicitly configured to resume. Session selection is confined to that directory. Pi-compatible JSONL files remain readable by Pi.

`newSession` replaces the active session. `cloneSession`, `rewind`, and `fork` require a valid persisted conversation; an empty session cannot be cloned or branched. `switchSession` rejects missing, outside-directory, empty, or SDK-incompatible files. Glass never rewrites or silently repairs an incompatible session. Recreate a session under the selected directory or choose another valid file.

Session state includes model provider and ID, thinking level, session ID/file/name, streaming and idle flags, and pending message count. Provider/model choice is Pi-owned. `glass agent models` and `glass.agent.models` list the SDK catalog; `provider/model` is required for selection. A missing model is an explicit error.

## CLI and adapter distinction

| Interface | Runtime | Persistence and controls |
|---|---|---|
| TUI Agent surface and daemon resident tools | Native `AgentRegistry` → `GlassPiRuntime` → `AgentSession` | Persistent worker/session; steering, follow-up, abort, approvals, model/thinking, session tree |
| `glass agent hello/prompt --harness local` | Deterministic local harness | Synchronous, no model provider, no Pi session |
| `glass agent prompt --harness pi` | Compatibility `PiHarness` launching `pi --mode rpc` | One-shot request; its CLI process and output are bounded; no resident ownership |
| `glass agent delegate codex/claude/opencode ...` | Temporary external child | Read-only default; no resident agent or session; see [Coding harness architecture](harness-architecture.md) |

Useful compatibility commands are:

```console
glass agent hello --harness pi --root .
glass agent prompt "inspect the project" --harness pi --root .
glass agent follow-up "check the result" --root .
glass agent abort --root .
glass agent new-session --root .
```

The compatibility adapter uses newline-delimited Pi command frames, caps one command at 1 MiB, waits at most 30 seconds for the protocol response and 120 seconds after response while waiting for settlement, retains at most 64 display events, and rejects a turn after 4,096 observed events. A non-interactive caller immediately denies extension UI requests unless unrestricted mode is active. These limits do not redefine the native 16 MiB framed protocol.

## Failure and recovery

| Failure | Result | Recovery |
|---|---|---|
| Node missing or older than 22.19.0 | Readiness `missing`/`incompatible`; spawn fails | Install a compatible Node and run `glass agent setup` |
| Managed SDK install or selected entry unavailable | Readiness `missing`/`incompatible`; no agent turn | Run `glass agent setup --update`, or select a valid `dist/index.js` |
| Missing/expired/invalid provider credentials | Readiness `missing`/`expired`/`unknown`; Pi remains unavailable | Run `glass agent setup --login` and `/login`, or set a supported API-key variable |
| SDK emits unexpected startup frame or closes IPC | Agent/operation fails; child is reaped | Inspect `lastError`, restart the failed agent, and verify selected SDK |
| Tool unavailable, stale, outside root, or lease denied | No tool effect; explicit error/evidence | Refresh workspace/browser state and issue a new governed call |
| Mutation approval denied or stale | Exact call is rejected; approval is not reusable | Inspect the frozen effect and approve a new call only if intended |
| Three unknown Glass tools in one turn | Adapter aborts the turn | Correct the tool name and start a new prompt/follow-up |
| Empty/incompatible/outside session | Session operation fails closed | Select a valid file under `.glass/pi-sessions` or create a new session |
| Agent runtime/event/token budget exceeded | Agent becomes failed and dependents do not run | Inspect evidence, adjust budget or prompt, then restart/reassign |

## Verification and evidence

The source tests provide these contracts:

- `pi_runtime::tests::request_mapping_uses_native_sdk_operations` checks operation names and context attachments.
- `runtime_asset_never_invokes_pi_rpc_cli` and `runtime_asset_registers_governed_custom_tools_without_builtins` distinguish native SDK startup from the compatibility CLI.
- `unknown_tool_call_recovers_then_aborts_once` checks the three-failure recovery boundary.
- `readiness_version_and_expiry_checks_are_deterministic` checks Node/version parsing and credential expiry classification.
- `browser_evidence_redacts_url_query_and_fragment` checks evidence redaction.
- `native_sdk_starts_and_reports_capabilities_when_installed` checks real SDK startup, naming, statistics, tree/messages, new/list, clone failure on empty state, and confinement failure.
- `agents::tests::independent_pi_sessions_schedule_dependencies_and_stream_state`, `invalid_agent_specs_and_failed_dependencies_fail_closed`, and `lossy_worker_queue_reports_dropped_events` check resident worker lifecycle, dependency state, and queue evidence.

For package-level evidence, `node --check crates/glass-dev/assets/pi-runtime.mjs` checks the embedded runtime and `npm --prefix packages/pi-runtime run check` checks the package adapter. Real SDK tests are conditional on an installed SDK and provider-free startup; they do not require a paid model request. Keep release claims tied to [release evidence](release-evidence.md) and the exact package lockfile.

## Related guides

- [Development Runtime](development-runtime.md)
- [Autonomous task DAGs](task-dag.md)
- [Coding harness architecture](harness-architecture.md)
- [MCP integration](mcp.md)
- [Local daemon](daemon.md)
- [Workspace trust](workspace-trust.md)
- [Release notes](releases/0.3.12.md)
- [Release evidence](release-evidence.md)
