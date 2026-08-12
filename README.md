# Glass

Glass is a local-first terminal workspace for developing, operating, and
verifying applications with humans and agents. It combines a bounded project
runtime, native terminal UI, local agent harness, browser intelligence, Model
Context Protocol (MCP) server, and reusable Rust library in one revision-aware
workspace.

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

Read the [Development Runtime guide](docs/development-runtime.md) for files,
editor ownership, processes, LSP, source/runtime graph, timeline, replay,
experiments, agents, and Neovim integration.

## Terminal workspace

Glass adapts by terminal geometry without treating a narrow screen as proof of
a slow network.

| Key | Desktop | Phone |
|---|---|---|
| `1` | Agent | Agent |
| `2` | Code | Code |
| `3` | App | App |
| `4` | Terminal | Tasks |
| `5` | Tasks | More |
| `6` | Git | - |
| `7` | Debug | - |

`Tab` and `Shift-Tab` move between views. `?` opens help, `:` or `/` opens
command discovery, and `Ctrl-L` redraws. Essential phone navigation uses
printable keys and does not require function keys or mouse input.

The browser view is structured-first. Continuous pixels are off by default.
Use `live on` only when visual steering is useful, and use an explicit
`screenshot PATH` when you need persistent visual evidence.

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

An occupied or disconnected CDP endpoint does not force the TUI to exit. In
the Browser view, inspect and choose an explicit recovery action:

```text
browser status
browser reconnect
browser launch --port auto --headless
browser targets 9222
browser attach --port 9222 2
browser semantic-only
```

Glass classifies the endpoint as free, verified CDP, unrelated HTTP, or
unknown. It probes again before attachment, refuses unrelated listeners, and
requires explicit target selection when multiple pages exist. Connecting to a
new target invalidates old semantic and visual revisions before tools become
available again.

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
glass agent models --root .
glass agent prompt "Explain the failing diagnostic" --harness pi --root .
```

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
providers from `models.json`; use `project pi models` and model selection from
the cockpit. `GLASS_PI_ONLINE_CATALOG=1` permits live catalog refresh,
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

Presentation is selected independently from layout:

- local balanced and smooth profiles request 30 and 60 FPS;
- verified fast remote links use higher bounded profiles;
- constrained or unknown remote links use 3/6/12 FPS profiles;
- Mosh remains semantic-only because it synchronizes terminal cells, not
  arbitrary graphics-protocol state;
- auto quality reduces capture scale before frame rate and suspends hidden
  browser acquisition.

Use Herdr when you want agent-aware persistent PTYs across SSH detach and
reattach. tmux remains compatible. For full-fidelity iPhone viewing, use one
of two private loopback paths:

- `safari` prints a stable application-server forwarding workflow;
- `browser remote-view open` serves the current BrowserSession through a
  random, revocable token and revision-bound input.

Neither path exposes CDP publicly or launches Safari on the remote machine.
The iOS SSH client owns the local port forward.

Follow [Mobile and remote development](docs/mobile-remote.md) for exact Herdr,
Mosh, terminal-graphics, Safari, Remote View, troubleshooting, and security
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
catalog](docs/examples.md), and [docs.rs API](https://docs.rs/glass-browser).

## TypeScript and Python clients

The repository contains dependency-light TypeScript and Python clients for
the Glass MCP control plane. They expose typed browser and Development Runtime
helpers, cursor-based event subscriptions, deadline-aware waits, process
health, mutation-lease scopes, and edit-and-verify flows.

They are repository clients for the 0.3.8 source line, not published npm or
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

| Item | 0.3.8 source status |
|---|---|
| Linux ARM64 | Native Chromium evidence recorded for the current release |
| Linux x86-64 | Native Pi, experiment, and Chromium evidence recorded for the release source |
| macOS x86-64 / ARM64 | Browser-free CI contract; native runtime certification pending |
| Windows | Browser-free CI plus native named-pipe daemon lifecycle; native PTY/browser certification pending |
| Chrome / Chromium | Supported browser families on environments with native evidence |
| Firefox / WebKit / Safari automation | Unsupported; iPhone Safari is a forwarded viewing client, not a Glass backend |
| `glass-browser 0.3.8`, `glass-dev 0.3.8` | Current release source; public registry state is recorded in release evidence |
| `0.3.5` | Previous published stable release |

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
