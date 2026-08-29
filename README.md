# Glass

Glass is a local-first coding workspace for developing, operating, and
verifying applications with humans and agents. Coding is the primary workflow:
the native terminal UI, local agent harness, files, editor, PTYs, tests,
diagnostics, and Git diff live together in one revision-aware workspace. Browser
intelligence is an integrated optional app surface for UI work and verification.


**Status: Current 0.3.14 source behavior.** The TUI, editor, Pi integration,
collaboration affordances, and terminal presentation notes below describe this
checkout; current-source changes are not claimed as published-release behavior.

The complete product is `glass-dev`, which installs both `glass` and
`glass-browser`. The focused `glass-browser` package provides the standalone
browser control plane and `glass_browser` Rust crate without the development
runtime.

```text
                               Glass workspace
                                      │
             ┌────────────────────────┼────────────────────────┐
             │                        │                        │
       project runtime          agent coordination       browser workspace
   files · editor · PTYs       local/Pi · MCP · leases   semantics · actions
   LSP · diff · timeline       revisions · attribution   Web IR · workflows
             │                        │                        │
             └────────────────────────┼────────────────────────┘
                                      │
                         CLI · TUI · MCP · Rust SDK
                                      │
                    local terminal · SSH/Mosh · iPhone
```

Glass does not host code, credentials, or browsers. It does not infer an
autonomous plan or silently act on a page. Project mutations stay inside the
selected root. Browser mutations require current evidence, policy authority,
and revision checks.

## Choose the product

| Package | Installed commands | Choose it when |
|---|---|---|
| `glass-dev` | `glass`, `glass-browser` | You want the complete development workspace, TUI, agents, MCP, project runtime, and browser control plane. |
| `glass-browser` | `glass-browser` | You want browser automation or the reusable Rust library without PTY, LSP, project, Pi, or Neovim dependencies. |

The packages intentionally share the `glass-browser` executable name and
cannot own it in the same Cargo installation root. Follow the tested
[installation and ownership-transition guide](docs/installation.md).
The command and feature notes below describe the current `0.3.14` source
checkout. Published docs.rs pages match the crate version they were built from
(`0.3.14` at last publication). They are not a substitute for checking this
checkout's TUI and CLI behavior.


## Install

Install the complete published product:

```console
cargo install glass-dev --locked
glass doctor
glass --help
glass-browser --help
```

Install only the browser control plane:

```console
cargo install glass-browser --locked
glass-browser doctor
```

Install the current checkout during development:

```console
cargo install --path crates/glass-dev --locked
```

Update the package that owns the command you invoke:

```console
glass update --dry-run
glass update
# Core-only installations expose the same operation:
glass-browser update
```

The updater preserves the detected Cargo install root and uses a locked Cargo
install. It never installs both packages or guesses between `glass-dev` and
`glass-browser`; see the [update contract and recovery
steps](docs/installation.md#update-a-cargo-installation).

Chrome or Chromium is required only for browser-backed operations. Project
inspection, file operations, task validation, Web IR inspection, policy
preflight, capability inspection, and many diagnostics are browser-free.

Use `glass install-chromium` when the platform has a supported Chrome for
Testing archive and no suitable system browser is available. Linux ARM64 uses
an automatically discovered system Chromium. Glass also places profiles for
confined Snap Chromium in its accessible Snap data directory automatically, so
the normal first run remains simply `glass`.

To remove Glass, use the [complete uninstall
procedure](docs/installation.md#fully-uninstall-glass). Cargo uninstallation
and deletion of retained profiles/runtime data are separate operations.

## Five-minute development loop

Open a project and inspect what Glass detected:

```console
cd /path/to/project
glass project inspect --root .
glass project files --root .
glass project diagnostics src/main.rs --root .
glass project diff --root .
```

`project inspect` reports the canonical root, project kind, detected commands,
and browser URL. `project files` returns a bounded tree with explicit limit,
truncation, ignored-directory, and skipped-symlink metadata. Reads and writes
cannot escape the canonical root.

Run a finite check in a real pseudo-terminal (PTY):

```console
glass project run check --command "cargo check" --wait --root .
```

Use the resident TUI or MCP server for a long-running development server. A
one-shot CLI process cannot own an interactive child after it exits.

Start the terminal workspace:

```console
glass
```

The workspace retains project, process, agent, browser, revision, and
attention state while the process is alive. A browser failure enters recovery;
it does not terminate the editor, PTYs, agent, or project session.

First-launch onboarding stays inside the TUI:

```text
glass → Trust (only when required) → Agent → Pi readiness → composer
                                      │
                         :agent setup / :agent setup login
                                      │
                         Enter sends → Glass Agent streams
```

On an untrusted project, Glass opens `Trust` first. `I` inspects, `O` opens
without trust, `1` trusts once, and `T` trusts the project; repository-controlled
execution stays blocked until a local decision. On `Agent`, type a message or
press `Enter` to begin. If Pi is not ready, the same flow queues `:agent setup`
to install or repair the pinned managed runtime; approve the one-use setup
request with `Enter`/`Y`, or cancel with `Esc`/`N`. Then use `:agent setup login`
to hand this terminal to Pi's `/login`; exit Pi to return to Glass. `:agent
update` refreshes the pinned runtime. `:agent doctor` and `:agent status` show
readiness without secrets.

Talk from any surface. `Ctrl-L` opens the shared composer dock. `Alt-A` docks
from the editor. Default mode is Agent. `Ctrl-Shift-A` cycles Ask, Plan, and
Agent. Ask and Plan are fail-closed for mutations. When the dock is open,
`Enter` sends the draft and leaves it available for the next prompt; `Esc`
closes it. `Ctrl-D` toggles steer mode, which interrupts a running turn, while
the default follow-up mode queues the next message. Sent prompts remain visible
as `YOU` messages while the resident agent streams `GLASS AGENT` text and tool
activity. A send failure restores the draft with an edit-and-retry message; a
busy background operation keeps new text in the composer. Every Pi mutation
pauses on a Glass approval card: `Enter`/`Y` approves that serialized call once
and `Esc`/`N` denies it.

Type in the dock to talk, `:` to search commands, and `a` to open this
surface's actions. The Terminal surface starts the detected development suite
with `:process start dev`; its action menu also exposes logs, input, stop, and
health. Tasks shows the workspace-local Agent checklist (persisted at
`.glass/todos/session.json`) and the overnight DAG. The More surface starts the private cockpit with `:cockpit
start`; Enter on `doctor` stays on More. Git loads the selected file's diff on
open, stages with Space, and commits with `c`. Git also offers read-only
`:github review` and confirmation-gated `:github ship TITLE`.

The resident workspace keeps project, process, editor, agent, browser, revision,
and attention state while Glass is alive. A browser failure enters recovery; it
does not terminate the editor, PTYs, agent, or project session.



Read the [Development Runtime guide](docs/development-runtime.md) for files,
editor ownership, processes, LSP, source/runtime graph, timeline, replay,
experiments, agents, and Neovim integration.

## Terminal workspace

Glass chooses layout from terminal geometry only: Auto is phone below 72
columns or 22 rows, compact below 118 columns or 32 rows, and desktop
otherwise. Override with `--tui-layout mobile|compact|desktop`; layout does not
prove network speed or graphics support.

```text
Desktop: [Agent Code App Terminal Tasks Git Debug More]  →  surface
          [navigation] [active surface / editor / browser] [context]
          [status and command/composer footer]

Phone:   [header]
         [one active surface: Agent | Code | App | Tasks | More]
         [status, or composer/command input]
```

| Key | Desktop | Phone |
|---|---|---|
| `1` | Agent | Agent |
| `2` | Code | Code |
| `3` | App | App |
| `4` | Terminal | Tasks |
| `5` | Tasks | More |
| `6` | Git | - |
| `7` | Debug | - |
| `8` | More | - |

The numeric keys are the stable route to each surface. On Desktop/Compact,
`c`, `v`, `w`, `g`, `d`, and `m` are mnemonic aliases for Code, App, Tasks, Git,
Debug, and More when the current surface has not claimed that key; for example,
`g` jumps to source on App and `c` commits on Git. Agent treats printable input
as composer text. Phone keeps the five numeric destinations above plus
`Tab`/`Shift-Tab`.

`Tab`/`Shift-Tab` move between available surfaces, as do Left/Right in the
normal surface view. `?` opens scrollable help, `:` opens surface-filtered
command discovery, `a` opens the current surface action menu outside Agent,
and `Ctrl-L` opens the chat dock. `Ctrl-P` opens files, `Ctrl-K` or
`Ctrl-Shift-P` opens the command palette, and `Ctrl-G` jumps to App while
keeping the dock open. On App, `Alt-Left`/`Alt-Right` navigate browser history
and `Ctrl-R` reloads. `Enter` activates the selected file/browser entity or
starts Agent interaction. Mouse is optional: click the dock to chat,
double-click to open, and right-click or long-press for actions. Printable
navigation works over SSH/Mosh.

On Code, the file list and editor preview are read-only until full-screen edit:
long preview lines wrap to the pane width on narrow terminals while retaining
their path-aware highlighting. Select a file and press `Enter` (or `i`) for the
full-screen editor. `Alt-W` toggles visual soft wrapping **off by default**;
when on, source lines wrap at whitespace where possible, continuation rows keep
their gutter alignment, and cursor, selection, and syntax highlighting remain
aligned. When off, horizontal scrolling follows source columns. In either
mode, arrows move, `Shift`+arrows select, `Ctrl-S` saves, `Ctrl-Z`/`Ctrl-Y`
undo/redo, and `Alt-A` asks Pi with the unsaved buffer attached. The editor
starts in INSERT. `Esc` returns to NORMAL. `Esc` from NORMAL on a clean buffer
leaves the editor. Unsaved files still ask: `S` save, `D` discard, `Q`
discard-and-quit, or `Esc`/`N` stay. `Ctrl-C` opens Glass quit confirmation
from editor input; if the unsaved-exit prompt is already open, its
save/discard/stay choices take priority.

The native editor also provides modal motions/operators, tree-sitter
textobjects with lexical fallback, FIM ghosts, LSP navigation/inlays, gutter
evidence marks, and bounded proposal hunk review.

The Code `REVIEW` panel shows open anchored comments, pending proposals, and
checkpoints. Use `:editor comment-selection TEXT`, `:editor comment PATH START
END TEXT`, `:editor comment-resolve ID`, `:editor propose PATH SUMMARY TEXT`,
`:editor proposals`, `:editor accept ID`, `:editor reject ID`, `:editor
checkpoint NAME`, and `:editor restore CHECKPOINT_ID`. Proposals require the
exact base content and become stale on conflicting edits; checkpoints and
proposals change resident buffers until `:editor save PATH` or `Ctrl-S`.
The browser view is structured-first. Continuous pixels are off by default;
use `:browser view` in the development TUI to toggle the selected backend, and
use `live on`/`live off` only in the standalone `glass-browser` TUI. Use an
explicit screenshot command when persistent evidence is needed.

## Source and diff rendering

The Code surface classifies source and diff content by path. Both paths use
the bundled `syntect` grammar when available, then deterministic manual
highlighting; an unknown format remains plain text rather than being guessed.
Path aliases cover TypeScript, Swift, Kotlin, Dart, and Dockerfile-like names.
Markdown headings and inline markup are styled directly, fenced code tracks its
declared language (including those aliases), and recognized Mermaid flowcharts
and sequence diagrams receive a terminal-native preview. A Mermaid or unknown
format that cannot be rendered safely remains readable source text.


Architecture and interaction details are in the [TUI
contract](docs/architecture/tui.md), [Development TUI
layout](docs/architecture/development-tui.md), and [mobile cockpit
design](docs/architecture/mobile-cockpit.md).

## Browser verification

Glass drives local Chrome or Chromium through a transport-neutral backend
contract. Raw Chrome DevTools Protocol (CDP) is the production backend.
WebDriver BiDi remains a bounded experimental backend. Unsupported operations
fail closed.

Start with structured evidence:

```console
glass observe --level interactive
```

An observation returns a browser revision and revisioned references such as
`r7:b42`. Guard the action with the observed revision:

```console
glass click r7:b42 --expected-revision 7
```

Glass resolves exactly one target, checks the expected revision, applies
policy, dispatches the input, and returns typed verification or recovery data.
An ambiguous locator does not become a best-effort click. A transport failure
after dispatch may be `indeterminate`; re-observe and reconcile instead of
blindly retrying.

For a long-lived browser session, use the TUI, MCP server, daemon, Rust API, or
a bounded workflow. Individual CLI invocations are process-scoped unless they
attach to an existing browser or resident service.

Normal observation does not capture screenshots, full DOM, cookies, form
values, network bodies, or evaluated JavaScript. Request sensitive or deep
evidence explicitly:

```console
glass screenshot --output evidence.png
glass dom
glass observe --form-values
glass diagnostics
```

Use these guides for the complete contract:

- [Semantic observation](docs/semantic-observation.md)
- [Actions and revisions](docs/actions.md)
- [Policy and confirmation](docs/policy.md)
- [Profiles and authenticated state](docs/profile-ergonomics.md)
- [Semantic execution and Glass Web IR](docs/semantic-execution.md)
- [Workflow definitions and recovery](docs/workflows.md)

## Browser recovery and targets
An occupied or disconnected CDP endpoint does not force the TUI to exit. On
the App surface, use `:browser targets` to open a searchable target picker or
`:actions` to choose **Targets**. The picker filters redacted title, URL, and
target ID text, then queues an explicit target selection after confirmation.

Browser startup from the TUI is headed and persistent by default:

```console
:browser start
:browser start --incognito --headless
```

The first command preserves an authenticated profile for human interaction;
the second is disposable. If the preferred port is occupied, Glass presents
attach, automatic-port, and retry choices while keeping the project and Pi
Agent surfaces available. Target inventory can be archived without selecting
or closing pages:

```console
glass archive-targets --output targets.json
```

Archives are bounded JSON with redacted target URLs and no cookies, form
values, or page content.

Read [Browser connection and Remote View](docs/architecture/browser-connection.md)
for the controller state machine and authority boundaries.

## Agents and collaboration

The Development Runtime includes a Glass-owned bounded tool gateway and actor
timeline. The deterministic local harness is always available:

```console
glass agent hello --root .
glass agent prompt "Inspect @workspace and @diagnostic" --root .
```

The native Pi SDK runtime uses a persistent `AgentSession` with raw built-in
tools disabled. Its only executable capability is the Glass-owned gateway into
bounded project, browser, workflow, and resident-runtime tools:

```console
glass agent hello --harness pi --root .
glass agent doctor
glass agent setup
glass agent setup --login
glass agent setup --update
glass agent models --root .
glass agent prompt "Explain the failing diagnostic" --harness pi --root .
```


Glass can also discover and launch installed external coding harnesses through
the same catalog used by the TUI:

```console
glass harness list
glass harness start codex --root .
```

It can delegate one bounded prompt to the supported temporary adapters without
making the process a resident session:

```console
glass agent delegate codex "review the current diff" --root .
glass agent delegate claude "explain the failing test" --root .
glass agent delegate opencode "map the project entrypoints" --root .
```

The TUI exposes the same operations as `:harness list`, `:harness start NAME`,
and `:harness delegate NAME PROMPT`. Delegation defaults to a read-only
harness sandbox and returns bounded structured output. Workspace writes require
`--sandbox workspace-write --allow-mutation --yes`; the resident Pi `delegate`
tool uses the same approval boundary.

`--update` forces a reinstall of Glass's pinned Pi SDK version; it does not
silently select an unreviewed upstream version.

For an intentionally unrestricted coding session, launch the cockpit with:

```console
glass --yolo
```

`--yolo` disables Pi/Glass tool confirmations for that process, automatically
accepts confirmation requests from loaded Pi extensions, grants browser policy
capabilities without confirmation, and loads trusted Pi resources plus all
their registered tools. It trusts the model, current project, shell commands,
and installed Pi extensions with the user's account. It does not disable
revision checks, workspace/daemon leases, explicit host denials, protocol
bounds, or result-size limits.

Nothing is downloaded during startup. `agent doctor` and `agent status` report
the resolved Node/SDK/auth/provider/session state without secrets; `agent
setup` explicitly installs the release-pinned SDK or selects an existing entry.
Tool availability follows the live workspace and its revisions, policy,
mutation leases, and PTY/browser ownership. Project mutations require the
applicable precondition and policy plus confirmation. In the resident TUI, each Pi mutation pauses on a
Glass-owned approval sheet; `Y`/Enter approves that exact serialized call once
and `N`/Esc denies it. Unanswered requests expire after 120 seconds, while
non-interactive Pi CLI requests deny them immediately. Prompt text, page content, secrets, and process
output are not copied into the persisted timeline.

The overrides are coding-harness complete: `read` supports bounded line paging,
`ls` supports path and result bounds, `grep` performs bounded literal UTF-8 text
search with path/glob/case/context controls, `find` supports `*`/`?` pathname
matching, `edit` applies an atomic exact-match edit set, and `bash` runs an
approved command for up to 300 seconds. Directory creation, rename, deletion,
diagnostics, verification, Git status, semantic inspection, Web IR, and task
planning are also available as explicit `glass_*` tools.

Pi retains its configured provider/model catalog, including supported custom
providers from `models.json`; use `glass agent models` or the Agent surface.
`GLASS_PI_ONLINE_CATALOG=1` permits live catalog refresh,
`GLASS_PI_PERSIST_SESSION=1` opts into Pi session persistence, and
`GLASS_PI_TRUSTED_RESOURCES=1` opts into ambient Pi context files, extensions,
skills, templates, and themes, and removes Glass's extension-tool allowlist so
those registered tools are selectable too. The last setting executes
user-installed code outside Glass's broker and should be used only for a trusted
machine and project.

External agents can use the CLI or MCP project tools and attach an attributed
actor. Conflicting file claims fail closed rather than silently overwriting
another actor's work.

## SSH, Mosh, Herdr, and iPhone

Force the phone layout when terminal geometry is reported incorrectly:

```console
glass --tui-layout mobile
```

The phone workspace uses purpose-built Agent, Code, App, Tasks, and More
destinations. Structured semantic state, project files, agent activity, task
evidence, and runtime health remain usable without continuous images.

Command discovery is centralized in the TUI:

- `:actions` opens guided launchers for the current surface;
- `:` searches every route, with the current surface's commands first;
- `Tab` completes route roots and `?` opens the complete keyboard guide.

The Command Center shows the safe next action and an example invocation, so
most workflows do not require memorizing the resident command inventory.
Presentation is selected independently from layout and transport. In the
development TUI, `--tui-live off` is the default. With `--tui-live auto` and
backend `auto`, Herdr is used only when detected; otherwise presentation stays
semantic-only. `--tui-live on` with backend `auto` uses bounded ANSI. An
explicit `--tui-live-backend kitty` emits Kitty terminal graphics, while
explicit `ansi` uses true-color half-block cells; an unavailable explicit
Herdr backend remains semantic-only. Quality `data`, `balanced`, and `smooth`
profiles target approximately 3, 6, and 12 FPS. Mosh and terminals without
graphics remain usable through semantic state.

Use `:browser view` in the development TUI to toggle the selected presentation
path; `live on`/`live off` are standalone `glass-browser` TUI commands, not
development-TUI commands. Herdr is optional and can own the persistent PTY for
detach/reattach; tmux remains compatible.
For iPhone viewing, use the development TUI's private Remote View:

```text
:browser remote-open
:browser remote-status
:browser remote-revoke
```

Glass prints a tokenized loopback URL and a hint equivalent to:

```console
ssh -N -L PORT:127.0.0.1:PORT USER@HOST
```

Run the forward from the iPhone-side network and open the printed local URL in
Safari. Remote View is not a standalone `glass-browser` command. The
application server, Remote View, and Chrome CDP must remain private.

Follow [Mobile and remote development](docs/mobile-remote.md) for exact Herdr,
Mosh, terminal-graphics, Remote View, troubleshooting, and security
procedures.


## MCP integration

Generate exact configuration instead of guessing command paths:

```console
glass mcp-config --client generic
glass mcp-config --client claude-code
glass mcp-config --client codex
```

The MCP server uses newline-delimited JSON-RPC on stdio. Stdout is protocol
only; diagnostics go to stderr. A client must initialize, negotiate Glass
schemas and capabilities, send `notifications/initialized`, and then call
tools. Requests and responses are bounded. Cancellation reaches waits and
pending operations without corrupting the resident browser session.

Start with [MCP integration](docs/mcp.md). Use the [complete MCP tool
catalog](docs/mcp-tools.md) for operation names, and use live `tools/list` as
the exact-version input-schema authority.

## Rust library

Embed only the browser control plane:

```toml
[dependencies]
glass = { package = "glass-browser", version = "0.3" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

The library owns browser launch/attach lifecycle, target and frame selection,
structured observation, revision-safe actions, stable Web IR, Task Protocol,
workflows, advisory knowledge, backend/surface contracts, presentation, daemon
and MCP integration. Project, PTY, LSP, agent, and Neovim runtime ownership is
provided by the separate `glass-dev` crate and installed `glass` product.

Owned sessions must call `BrowserSession::close().await` so Chrome can flush a
persistent profile before process fallback. Attach sessions never own or close
the external browser.

Read the [Rust SDK guide](docs/rust-sdk.md), [runnable example
catalog](docs/examples.md), [glass-browser docs.rs](https://docs.rs/glass-browser),
and [glass-dev docs.rs](https://docs.rs/glass-dev). Those docs.rs pages describe
published Rust packages. Published `0.3.14` pages match that crate version;
this repository checkout can contain newer TUI and MCP surfaces.

## TypeScript and Python clients

The repository contains dependency-light TypeScript and Python clients for
the Glass MCP control plane. They expose typed browser and Development Runtime
helpers, cursor-based event subscriptions, deadline-aware waits, process
health, mutation-lease scopes, and edit-and-verify flows.

They are repository clients for the 0.3.14 source line, not published npm or
PyPI packages and not browser runtimes:

- [TypeScript client](clients/typescript/README.md)
- [Python client](clients/python/README.md)

## State, privacy, and ownership

| State | Owner | Persistence |
|---|---|---|
| Project buffers and PTYs | active Glass project session | process lifetime; saves explicitly mutate project files |
| Development timeline | project-scoped local data | bounded and actor-attributed; prompt text and process output excluded |
| Browser profile | named Glass profile or external attached Chrome | persistent for named profiles; disposable for incognito |
| Semantic observation | BrowserSession | bounded in memory and invalidated by navigation/target/reconnect changes |
| Screenshot, DOM, PDF, diagnostics | explicit caller operation | returned/written only when requested |
| Knowledge and snapshots | profile/workspace-scoped store | bounded, validated, explainable, and explicitly manageable |
| Reconnect capsule | project-scoped local data | non-sensitive navigation/control metadata only |
| Remote View frames | current BrowserSession | newest-frame memory only; never persisted by the service |

Keep CDP, daemon sockets, Remote View, and development servers on trusted local
interfaces. Use SSH forwarding instead of public binds. Treat profiles,
screenshots, DOM, cookies, evaluated output, logs, and exported knowledge as
sensitive.

Read [Security](SECURITY.md), [Policy](docs/policy.md), and [Ownership and
compatibility](docs/ownership.md) before deploying authenticated or multi-agent
workflows.

## Support and evidence

| Item | 0.3.14 source status |
|---|---|
| Linux ARM64 | Native Chromium evidence for exact `0.3.14` source pending; tracked in [release evidence](docs/release-evidence.md) |
| Linux x86-64 | Native Pi, experiment, and Chromium evidence for exact `0.3.14` source pending; tracked in [release evidence](docs/release-evidence.md) |
| macOS x86-64 / ARM64 | Browser-free CI contract; exact `0.3.14` native runtime certification pending and tracked in [release evidence](docs/release-evidence.md) |
| Windows | Browser-free CI plus native named-pipe daemon lifecycle capability; exact `0.3.14` native PTY/browser certification pending and tracked in [release evidence](docs/release-evidence.md) |
| Chrome / Chromium | Supported browser families on environments with native evidence |
| Firefox / WebKit / Safari automation | Unsupported; iPhone Safari is a forwarded viewing client, not a Glass backend |
| `glass-browser 0.3.14`, `glass-dev 0.3.14` | Current release source; public registry state is recorded in release evidence |
| `0.3.13` | Previous published stable release |

A source build, cross-compilation, or browser-free CI run is not native browser
certification. Read the [feature-parity matrix](docs/feature-parity.md),
[platform certification guide](docs/ci-platform-certification.md), and
[release evidence](docs/release-evidence.md) for exact claim boundaries.

## Documentation map

| Goal | Start here | Deep reference |
|---|---|---|
| Install, update, switch packages, or uninstall | [Installation](docs/installation.md) | [Release checklist](docs/release-checklist.md) |
| Learn the complete product | [Getting started](docs/getting-started.md) | [Feature reference](docs/features.md) |
| Develop in the terminal workspace | [Development Runtime](docs/development-runtime.md) | [Development TUI architecture](docs/architecture/development-tui.md) |
| Use Glass over SSH or iPhone | [Mobile and remote](docs/mobile-remote.md) | [Connection/presentation policy](docs/architecture/connection-presentation.md) |
| Automate a browser safely | [Semantic observation](docs/semantic-observation.md) | [Automation contracts](docs/architecture/automation.md) |
| Build workflows and semantic tasks | [Semantic execution](docs/semantic-execution.md) | [Workflow definitions](docs/workflows.md) |
| Connect an MCP agent | [MCP integration](docs/mcp.md) | [MCP tool catalog](docs/mcp-tools.md) |
| Embed the Rust crate | [Rust SDK](docs/rust-sdk.md) | [Examples](docs/examples.md) |
| Operate profiles, policy, or daemon | [Profiles](docs/profile-ergonomics.md) | [Policy](docs/policy.md), [daemon](docs/daemon.md) |
| Understand architecture | [Architecture index](docs/architecture/README.md) | [Documentation index](docs/INDEX.md) |

The role-based [documentation index](docs/INDEX.md) routes every current guide,
architecture contract, operator runbook, SDK reference, and historical release
record.

## Develop and verify

```console
cargo fmt --all -- --check
scripts/check-rust-workspace.sh test
scripts/check-rust-workspace.sh clippy
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked
python3 scripts/check-documentation-coverage.py
python3 scripts/check-documentation-depth.py
python3 scripts/check-release-documentation.py
```

Run the native browser suite only in an environment with supported Chromium:

```console
GLASS_E2E=1 cargo test -p glass-browser --all-features \
  --test browser_smoke --locked -- --nocapture --test-threads=1
```

See [Contributing](CONTRIBUTING.md) for repository structure, focused tests,
security rules, documentation contracts, and conventional commits.

## License

Glass is licensed under the [MIT License](LICENSE).
