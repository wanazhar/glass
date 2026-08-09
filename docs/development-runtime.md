# Development Runtime

The Development Runtime is the browser-independent foundation of the complete
`glass-dev` product. It owns a canonical project, bounded files and buffers,
real pseudo-terminal (PTY) processes, language-server state, attributed events,
source/runtime links, experiments, and agent tools. The TUI, CLI, MCP server,
TypeScript client, and Python client call the same Rust contracts.

`glass-browser` excludes this runtime by default. `glass-dev` enables the
one-way `development-runtime` feature and installs both `glass` and the
browser-only `glass-browser` entry point.

## Start a project session

Install and inspect without launching Chrome:

```console
cargo install glass-dev --locked
cd /path/to/project
glass project inspect --root .
```

Glass canonicalizes the root before it creates state. It detects Rust,
JavaScript/TypeScript, Python, and Go markers; package manager, build system,
formatter, framework, available language servers, default commands, and an
optional configured browser URL. `glass.toml` takes precedence over
`.glass.toml`. Detection never executes a command.

Start the resident workspace with `glass`. Use the TUI or MCP when the runtime
must retain editor buffers, PTYs, language servers, agent sessions, browser
context, or event cursors across operations. Use the CLI for finite,
browser-free inspection and `--wait` process work.

## Ownership and lifecycle

```text
canonical project root
        │
        └─ ProjectWorkspace
             ├─ tree snapshot/cache
             ├─ native editor buffers and claims
             ├─ ProcessManager (maximum 32 owned jobs)
             ├─ LanguageServiceManager
             ├─ source/runtime graph
             ├─ attributed bounded timeline
             └─ optional local or Pi harness
                    │
                    └─ optional attached Browser Workspace context
```

The one-shot CLI creates a workspace for one command and closes it. A stdio
MCP server retains a bounded canonical-root registry for that server process.
The daemon shares its own registry across authenticated local clients. The TUI
owns one resident project workspace. Registries retain at most eight projects
and evict an idle project after 30 minutes.

`project.session.status` reports the resident MCP lifecycle.
`project.session.detach` stops ownership and removes the registry entry. MCP
stdio shutdown, daemon shutdown, or idle eviction also closes the session.
Closing a workspace shuts down language servers and terminates owned process
trees; it does not delete project files.

## Project files and tree

List the bounded tree:

```console
glass project files --root .
```

The result contains entries plus `limit`, `truncated`, ignored directories,
and skipped symlinks. The default maximum is 2,048 entries. Glass excludes
generated or vendor roots such as `.git`, `target`, `node_modules`, and
`.glass` before they consume the scan budget. Symlinks are not followed into
unknown or external trees.

The resident tree is cached and invalidated by Glass file mutations. A caller
must treat `truncated: true` as an incomplete inventory, not an empty or fully
scanned suffix.

Read, search, and mutate:

```console
glass project read README.md --root .
glass project search BrowserSession --root .
glass project edit notes.txt --content "reviewed\n" --root .
glass project mkdir docs/drafts --root .
glass project rename notes.txt docs/drafts/notes.txt --root .
glass project delete docs/drafts/notes.txt --yes --root .
```

Relative paths are limited to 512 bytes. Absolute paths, parent traversal, and
symlink escape fail with `PathOutsideWorkspace`. Delete requires explicit
confirmation and removes only a file or empty directory. Glass does not
recursively delete a project tree.

## Native editor and conflict rules

Opening a file creates a bounded editor buffer with the disk fingerprint and
an actor claim. Saves write atomically. A save fails when:

- the file changed externally after the buffer opened;
- another actor owns a conflicting edit claim;
- the path resolves outside the canonical root;
- the content or retained buffer limits are exceeded; or
- the filesystem operation fails.

The failure does not overwrite disk. Reopen or reread the file, compare the
new content, reconcile the actor claim, and apply a new edit.

TUI editor keys:

| Key | Action |
|---|---|
| arrows | move the cursor |
| text, `Enter`, `Tab` | insert content |
| `Backspace`, `Delete` | remove content |
| `Ctrl-S` | atomically save with actor attribution |
| `Ctrl-Z`, `Ctrl-Y` | undo or redo in the current buffer |
| `Esc` | return focus to the command bar |

Glass-native rendering remains the state owner. Neovim can act as an optional
editing engine; it does not own project, browser, process, graph, actor, or
timeline state.

## Processes and PTYs

Start a finite job from the CLI:

```console
glass project run check --command "cargo check" --wait --root .
```

Start a long-running server in the TUI command bar:

```text
project run dev npm run dev
```

Or use MCP `project.process.start` with `wait: false`. The resident project
session then owns the process.

Process contracts:

- maximum 32 managed jobs per project session;
- a real PTY for terminal-compatible input/output;
- bounded retained output rather than an unbounded log;
- explicit running, exited, failed, degraded-poll, and stopped state;
- input and resize only for a live owned job;
- unique process names; and
- graceful interrupt/terminate followed by bounded hard-kill escalation.

On Unix, Glass owns a process group so descendants cannot outlive the
workspace. On Windows, the development feature uses a kill-on-close Job
Object. A polling failure is returned as degraded state; it is not silently
discarded.

Use `glass project process --help` for list/start/stop/restart/remove/input/
resize/output syntax. Stop a job before removing its record. An exited process
can retain bounded output until explicit removal or workspace eviction.

## Tests, lint, and detected commands

Run detected finite commands:

```console
glass project test --root .
glass project lint --root .
```

Detection supplies defaults only when project evidence exists. A configured or
explicit command remains the authority. Glass records start, completion,
status, and actor evidence; it does not reinterpret a failing exit status as a
passing test because diagnostics were empty.

Use a named `project run` command for tools outside the detected test/lint
contract. Treat command strings as local code execution under the current user
and project trust boundary.

## Diagnostics and persistent LSP

Request diagnostics for one path:

```console
glass project diagnostics src/main.rs --root .
```

In a resident workspace, one language-service manager owns the server process
and JSON-RPC channel. It sends:

1. `initialize` and `initialized` once;
2. monotonic `didOpen` and `didChange` document versions;
3. `didSave` after an attributed save;
4. `didClose` when the buffer closes; and
5. `shutdown` followed by `exit` before process reap.

The service also supports bounded hover, definition, references, document
symbols, formatting, and rename operations where the selected server provides
them. Responses are bounded and matched to request IDs. Missing executables,
protocol failures, timeouts, and malformed responses are explicit errors;
Glass does not fabricate diagnostics or silently start a new server for each
request.

Detected server names are `rust-analyzer`, `typescript-language-server`,
`pyright-langserver`, and `gopls`. Installation is external to Glass.

## Source/runtime graph and live evidence

Discover and inspect links:

```console
glass project graph discover --root .
glass project graph entity action.checkout.submit --root .
glass project graph source src/checkout.rs --root .
glass project link action.checkout.submit src/checkout.rs \
  --start-line 40 --end-line 72 --root .
```

Every link has direction, provenance, confidence, and evidence. An explicit
`data-glass-entity="..."` marker is the strongest built-in source-to-runtime
bridge. Framework names, file proximity, or matching strings do not become
confirmed runtime ownership without evidence.

A source save creates a pending live-update state. It becomes confirmed only
after an attached browser produces a strictly newer semantic revision with
compatible evidence. Without a browser, the result remains pending rather
than claiming that hot reload succeeded.

`project diff` combines code, process, semantic-link, workflow, and explicit
visual status. Visual state is `not-captured` until a screenshot or comparison
is requested.

## Timeline, inbox, and replay

Every meaningful project operation records an attributed, bounded event. The
timeline persists under the platform local-data `glass/development` root using
a hash of the canonical project path. It stores event metadata and bounded
payloads. It does not persist prompt text, authored secret values, complete
process output, page content, cookies, or pixels.

Inspect it:

```console
glass project timeline --root .
glass project replay --root .
```

The mobile attention inbox derives `needsAttention`, `running`, and `recent`
cards from the same timeline. MCP `project.inbox` exposes the bounded
projection. Replay reconstructs a bounded sequence of attributed revisions;
it does not re-execute shell commands or browser input.

Reconnect capsules persist only control-plane identity, selected view/cursor,
target and revision metadata, attention title, and live preferences. They do
not claim that a PTY survived a process or machine crash.

## Agents and actor authority

Attach an external actor:

```console
glass project attach reviewer --root .
```

The actor appears in the attributed timeline. Actor identity is evidence, not
automatic mutation authority. File claims, project revisions, browser
revisions, policy, confirmation, and the shared mutation lease remain
independent checks.

The deterministic local harness resolves bounded references such as
`@workspace`, `@diagnostic`, `@run:last`, `@file:PATH`, `@entity:ID`, and
`@replay:INDEX`:

```console
glass agent hello --root .
glass agent prompt "Inspect @workspace and @diagnostic" --root .
```

The optional Pi adapter owns one line-delimited RPC session in a resident TUI.
It supports prompt, steer, follow-up, model discovery/selection, thinking
level, abort, and new session. Pi is not required for project or browser use.

Glass launches supported Pi versions offline and ephemeral, with built-in
tools, ambient extensions, skills, prompt templates, themes, context-file
discovery, and session persistence disabled. The embedded
`pi-glass-system.md` prompt makes Glass revisions, structured-first browser
evidence, privacy, narrow-terminal output, and truthful effect reporting part
of every turn without importing arbitrary user or project Pi configuration.

A Glass-owned extension exposes nine read-only tools: bounded file read/list/
search, Git/runtime-impact status, semantic entity links, Web IR inspect/diff/
continuity, and value-free Task Protocol compilation. Each call crosses the
same Rust gateway used by the local harness. Cross-process requests use
mode-0600, size-bounded, one-use files that the broker removes immediately
after reading. Arguments and results are schema-validated and capped. Pi file
or process mutation tools are intentionally absent until a per-call approval
can be represented in the Glass TUI; a model response is not confirmation.

Pi's prompt response only acknowledges queueing. Glass therefore continues
consuming JSONL events until `agent_settled` (an earlier `agent_end` may still
be followed by retry, compaction, or queued continuation), forwards bounded
message/tool/completion events to the Agent view, and drops token-level
`message_update` noise before it can cause terminal redraws. The resident
worker multiplexes commands with output at 50 ms or better, so `steer`,
`follow-up`, and `abort` can be delivered while an agent turn is running.
One-shot CLI prompts wait for the settled agent result; use the resident TUI
for controls that target an active turn. RPC records are capped at 512 KiB, the
reader applies backpressure after 32 queued records, and one-shot responses
retain only the newest 64 display-worthy events.

An attached Browser Workspace adds ephemeral target, origin, semantic summary,
browser revision, workflow, memory-scope, and authority references. Browser
tools are absent while detached or recovering. Context never bypasses lease,
revision, policy, or confirmation requirements.

## Experiments

Create one isolated Git worktree experiment:

```console
glass project experiment create alternative --port 3101 --root .
```

Glass creates a `glass/experiment/NAME` branch and a sibling
`.glass-worktrees/REPOSITORY/NAME` worktree. The experiment records its dev
port, browser URL, and agent thread, and can collect bounded code/test/semantic/
workflow comparison evidence.

The runtime does not automatically delete an experiment branch. Before a full
uninstall or manual cleanup, inspect `git worktree list`, remove each unwanted
worktree with `git worktree remove /exact/path`, and decide separately whether
to delete its branch.

## Neovim integration

Probe the installed executable and embedded RPC path:

```console
glass project neovim probe --root .
```

The probe runs a normal executable check and a real
`nvim --embed --headless --clean` Msgpack-RPC buffer create/set/get round trip.
Headless Lua stdout is not accepted as RPC evidence.

`glass project neovim start` launches compatibility Mode A in a managed PTY.
The 0.3.3 design keeps Glass-native rendering and state ownership while using
Neovim as an optional editor engine. Probe failure does not disable the native
editor.

## CLI, MCP, TUI, and client mapping

| Capability | CLI | MCP | TUI |
|---|---|---|---|
| detection/tree/read/search | `project inspect/files/read/search` | `project.*` query tools | Project view and palette |
| edit/save | `project edit` | revision/actor-bound edit tools | native editor, `Ctrl-S` |
| process lifecycle | `project run/process/test/lint` | resident process tools | Process view |
| diagnostics/LSP | `project diagnostics` | diagnostic and editor tools | Project diagnostics card |
| graph/diff | `project graph/link/diff` | graph and verification tools | Diff view |
| events/replay/inbox | `project timeline/replay` | cursor subscription and inbox | Overview/Agent views |
| harness | `agent ...` | agent gateway tools | resident Agent view |

TypeScript and Python clients wrap MCP. They do not bypass MCP initialization,
bounds, leases, or server ownership.

## Failure and recovery matrix

| Failure | Effect | Safe recovery |
|---|---|---|
| path escapes root | no filesystem mutation | choose a relative in-root path; do not follow an external symlink |
| external buffer change | save refused | reread, compare, reopen, then apply a new edit |
| actor claim conflict | save refused | reconcile/release the claim and retry from the current revision |
| process poll degraded | process state uncertain; output retained | inspect again, then stop the owned job if necessary |
| language server missing/crashed | no fabricated diagnostics | install/restart the server and reopen the document |
| browser disconnect | browser tools/revisions invalidated; project remains alive | reconnect, launch, attach, or choose semantic-only mode |
| MCP project eviction | resident buffers/process ownership closed | reattach the canonical root and restart required processes |
| interrupted mutation | effect may be incomplete | inspect timeline/current state and reconcile before retry |

## Privacy and retained state

- Project files change only through explicit file commands or editor saves.
- PTY commands execute with the current user's local authority.
- Timeline and reconnect data are bounded and exclude prompt/source/output
  bodies by contract.
- Browser semantics are structured-first; screenshots and visual comparisons
  are explicit.
- Agent gateway payloads and results are size-bounded and record digests rather
  than sensitive values.
- Full uninstall requires separate package and retained-state cleanup. See
  [Fully uninstall Glass](installation.md#fully-uninstall-glass).

## Related guides

- [Complete feature reference](features.md)
- [CLI reference](cli.md)
- [MCP integration](mcp.md)
- [Mobile and remote development](mobile-remote.md)
- [Development TUI architecture](architecture/development-tui.md)
- [Semantic execution](semantic-execution.md)
- [Security](../SECURITY.md)
