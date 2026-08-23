# Development Runtime

The Development Runtime is the browser-independent foundation of the complete
`glass-dev` product. It owns a canonical project, bounded files and buffers,
real pseudo-terminal (PTY) processes, language-server state, attributed events,
source/runtime links, experiments, and agent tools. The TUI, CLI, MCP server,
TypeScript client, and Python client call the same Rust contracts.

`glass-dev` owns this runtime directly and depends one-way on ordinary public
`glass-browser` APIs. There is no browser feature bridge. The package installs
both `glass` and the browser-only `glass-browser` entry point.

## Start a project session

Install and inspect without launching Chrome:

```console
cargo install glass-dev --locked
cd /path/to/project
glass project inspect --root .
```

For later registry releases, `glass update --dry-run` previews the resolved
package and root and `glass update` updates the existing `glass-dev` owner. See
[Installation and operations](installation.md#update-a-cargo-installation) for
the source-provenance and package-transition rules.

Glass canonicalizes the root before it creates state. It detects Rust,
JavaScript/TypeScript, Python, and Go markers; package manager, build system,
formatter, framework, available language servers, default commands, and an
optional configured browser URL. `glass.toml` takes precedence over
`.glass.toml`. Detection never executes a command.

Start the resident workspace with `glass`. Use the TUI or MCP when the runtime
must retain editor buffers, PTYs, language servers, agent sessions, browser
context, or event cursors across operations. Use the CLI for finite,
browser-free inspection and `--wait` process work.

Opening a project starts in `Untrusted` unless its current filesystem identity
matches the Glass-owned external trust store. Static files, search, Git
metadata, configuration review, and manual browser use remain available;
project hooks, skills, commands, tools, tests, LSP/DAP overrides, Pi, kernels,
and experiments do not execute before a local decision. The TUI presents the
decision before activation on desktop, compact, and phone layouts. See
[Workspace trust](workspace-trust.md) for inspection, persistence, and
authority details. `--yolo` never bypasses workspace trust.

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
| text, `Enter` | insert content |
| `Backspace` | remove content |
| `Ctrl-S` | atomically save with actor attribution |
| `Ctrl-Z`, `Ctrl-Y` | undo or redo in the current buffer |
| `Esc` | leave editor mode and return to Code navigation |

`Tab` remains product-destination navigation outside the editor, and `Delete`
has no editor mutation binding. Glass-native rendering remains the state owner.
Neovim can act as an optional editing engine; it does not own project, browser,
process, graph, actor, or timeline state.

## Source and diff rendering

The Code view and inline Git diff select syntax by path. Syntect provides the
bundled grammar when one is available; unknown paths use deterministic manual
rendering or plain text rather than content-based language detection. The
supported path aliases include TypeScript (`.ts`, `.tsx`, `.mts`, `.cts`) via
JavaScript, Swift and Dart via C++, Kotlin (`.kt`, `.kts`) via Java, and
Dockerfile-like names via the shell grammar.

Markdown (`.md`, `.markdown`, `.mdx`, and README names) styles headings, lists,
links, and inline code. Fenced blocks delimited by backticks or tildes select
the language token and highlight TypeScript, Swift, Kotlin, or Dart aliases.
Mermaid (`.mmd` and `.mermaid`) renders supported flowcharts, graphs, and
sequence diagrams as deterministic terminal diagrams; unsupported forms stay
as styled source.

Diff rendering follows each file path from its `---`/`+++` headers, displays
old/new line numbers and add/remove backgrounds, and applies that path's
grammar to changed lines. A path without a grammar retains the plain/manual
fallback.

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

For a fresh server in the resident TUI, start it before requesting
diagnostics. Enter these commands in the command palette:

```text
lsp start rust-analyzer rust-analyzer
lsp diagnostics rust-analyzer src/main.rs
```

The one-shot `project diagnostics` convenience command uses the Rust
diagnostics path and starts its dedicated client as needed:

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

The native Pi SDK runtime owns one persistent `AgentSession` per resident agent.
It supports prompt, steer, follow-up, model discovery/selection, thinking
level, abort, resume, clone, fork, compaction, and history. Pi is not required
for project or browser use.

The TUI Agent surface keeps this path in one terminal: `Enter` or `i` opens
the resident Pi composer, `s`/`u` queue runtime setup or update, and `l` hands
the terminal to Pi for interactive provider login. `review` pre-fills an
evidence-aware review prompt without editing files. `harness list` discovers
fixed external coding-harness binaries on `PATH`; `harness start NAME` hands
the terminal to one installed harness and resumes Glass after it exits. The
handoff never interpolates a user-supplied shell command and remains behind
workspace trust.

Glass loads the release-pinned SDK through private length-prefixed IPC, with
built-in tools, ambient extensions, skills, prompt templates, themes, and
context-file discovery disabled. The embedded
`pi-glass-system.md` prompt makes Glass revisions, structured-first browser
evidence, privacy, narrow-terminal output, and truthful effect reporting part
of every turn without importing arbitrary user or project Pi configuration.

A Glass-owned SDK tool exposes the workspace router. Every call crosses the
same Rust gateway used by the local harness, with schema/result bounds, actor
attribution, revision guards, confinement, mutation authority, leases, and
browser ownership preserved.

For a mutation, the trusted extension serializes the complete tool call before
asking. Glass blocks that tool while its confirmation sheet shows the
tool name and bounded effect evidence: path/name, redacted command preview, byte
counts, replacement counts, and short SHA-256 evidence for content or commands. `Y`/Enter or the
Approve once button authorizes the same frozen call once. `N`/Esc denies.
Requests expire after 120 seconds, duplicate/concurrent
and stale responses fail closed, and an approval is consumed once. It does not
authorize a retry, changed arguments, another tool, or another session. The
one-shot CLI adapter has no interactive host, so it immediately denies every UI
request instead of hanging. Raw patch content and secret-looking environment
assignment values are not placed in status or audit messages. Exact edit blocks
must each match once against the same original file and may not overlap; the
combined write is atomic and fails if the file changes after opening.

The SDK receives no second unconfined filesystem or shell path. Coding,
process, browser, and workflow calls all return to the resident workspace actor,
so persistent ownership and revision/lease checks are shared with CLI, TUI,
daemon, and MCP callers.

Provider/model choice is Pi-owned: built-in providers and supported
`models.json` custom providers remain available. Glass defaults to cached model
catalogs, an ephemeral Pi session, and no ambient resources. Operators may set
`GLASS_PI_ONLINE_CATALOG=1`, `GLASS_PI_PERSIST_SESSION=1`, or
`GLASS_PI_TRUSTED_RESOURCES=1`. Trusted resources load ambient context files,
extensions, skills, prompt templates, and themes; because extensions execute
local code outside the broker, that opt-in is equivalent to trusting those
installed resources with the user's account. It also removes the extension-tool
allowlist, making every tool registered by those extensions selectable. Pi's raw
built-in tools remain off because Glass overrides their standard coding names.

### Unrestricted launch

`glass --yolo` is the explicit no-approval development mode. For the lifetime
of that Glass process it:

- skips the Glass-owned confirmation sheet for every Pi mutation;
- automatically accepts any Pi extension `confirm` RPC that still occurs;
- authorizes private broker mutations without `--allow-mutation --yes` being
  supplied manually;
- loads ambient Pi context, extensions, skills, prompt templates, themes, and
  every tool registered by those extensions; and
- treats browser policy capabilities as explicitly allowed rather than
  confirmation-required.

The phone and desktop cockpit keep a visible `YOLO` marker/capability warning.
This mode trusts the model, prompt context, project, commands, and installed Pi
extensions with the current operating-system account. It intentionally does
not remove stale-revision checks, browser/workspace mutation leases, explicit
host denials, path checks performed by individual file tools, JSON/RPC limits,
timeouts, or result bounds. Shell commands and trusted extensions are arbitrary
local code and are not confined to the project root.

Pi's prompt response only acknowledges queueing. Glass therefore continues
consuming SDK events until `agent_settled` (an earlier `agent_end` may still
be followed by retry, compaction, or queued continuation), forwards bounded
message/tool/completion events to the Agent view, and drops token-level
`message_update` noise before it can cause terminal redraws. The resident
worker multiplexes commands with output at 50 ms or better, so `steer`,
`follow-up`, and `abort` can be delivered while an agent turn is running.
One-shot CLI prompts wait for the settled agent result; use the resident TUI
for controls that target an active turn. IPC frames are capped at 16 MiB, the
reader applies bounded backpressure, and one-shot responses retain only the
newest display-worthy events.

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
