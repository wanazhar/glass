# glass-dev

`glass-dev` installs `glass` and `glass-browser`, the full terminal-native Glass
coding environment. The coding workspace is the primary surface: it combines a
bounded project runtime, Ratatui editor, local agent harness, MCP server, PTYs,
tests, diagnostics, Git diff, and a phone-oriented remote cockpit. The browser
control plane is an integrated optional app surface for UI work and verification.

Use `glass-browser` instead when you need only the browser CLI or reusable Rust
library.

## Install and diagnose

```console
cargo install glass-dev --locked
glass doctor
glass --help
glass-browser --help
```

For subsequent Cargo registry releases, preview and apply an ownership-aware
update with `glass update --dry-run` and `glass update`. Invoking
`glass-browser update` from this package updates the same `glass-dev` owner;
it does not install the core-only package. Use `--version VERSION` to pin a
release and `--force` only for an intentional reinstall.

`glass-dev` is the umbrella package and owns both installed commands. Do not
also install `glass-browser` into the same Cargo home unless you intentionally
want that package to replace the shared `glass-browser` executable.

The public Rust API is documented at [docs.rs/glass-dev](https://docs.rs/glass-dev);
use the [CLI reference](https://github.com/wanazhar/glass/blob/main/docs/cli.md)
for installed command behavior. This guide follows the current `0.3.12` source
checkout; published `0.3.12` docs.rs pages describe released API artifacts and
may not include the checkout's newest TUI surfaces.


Chrome or Chromium is needed only for browser-backed operations. Project
inspection, files, search, local harness, Task Protocol/Web IR operations,
policy preflight, and many diagnostics are browser-free.

## Terminal workspace

```console
cd /path/to/project
glass
```

Desktop mode opens the Development coding workspace by default. Its eight
destinations are Agent, Code, App, Terminal, Tasks, Git, Debug, and More. The
workspace owns a bounded file tree, native editor, PTY processes, diagnostics,
actors, timeline, source/runtime graph, diff, replay, and optional Pi/Neovim
adapters.

Start from the CLI without opening Chrome:

```console
glass project inspect --root .
glass project files --root .
glass project run check --command "cargo check" --wait --root .
glass project diagnostics src/main.rs --root .
glass project diff --root .
glass agent prompt "read README.md" --root .
```

Project paths remain inside the canonical root. Reads, buffers, PTY output,
events, search results, and retained sessions are bounded. Saves are atomic and
actor-attributed. External changes and conflicting edit claims fail closed.

## SSH, Mosh, and iPhone

Narrow remote terminals automatically use the phone layout:

```console
glass --tui-layout mobile
```

Use `1`–`5` or `Tab` for Agent, Code, App, Tasks, and More. App uses the same
revision-bound semantic selection as the desktop and standalone browser
workspace. Continuous pixels remain off by default.

First-launch agent setup stays inside the TUI. On the Agent surface, type a
message or press `Enter` to open the composer. Use `:agent setup` to install or
repair the pinned managed Pi runtime, `:agent update` to refresh it, and
`:agent setup login` to open Pi `/login` in the terminal. `:actions` opens the
guided command center; `?` opens contextual help. Agent mutations pause on an
inline Glass approval card: `Enter` approves the exact call once and `Esc`
denies it.

On the App surface, use `:browser targets` to open a searchable page-target
picker without changing the active page. Type a title, URL, or target ID;
arrow keys select a result and `Enter` queues an explicit, one-use target
selection. Use `:browser type TARGET TEXT` to type into the selected page.

The Terminal surface is the dev-suite entrypoint: use `:process start dev` to
start the detected project command behind the same confirmation card, while
`:actions` exposes process start, logs, input, and health actions. Browser start
from the TUI is headed and persistent by default so existing authenticated
profiles remain usable; use `:browser start --incognito --headless` for a
disposable automation session.

The Agent surface also owns the task loop: use `:task list` to inspect work,
`:task create TITLE PROMPT` to queue a verified task, and `:task resume TASK_ID`
to continue a paused task. From `More`, `:cockpit start` opens a tokenized
loopback-only URL for remote inspection of the same workspace; `:cockpit stop`
closes it with the TUI. On `Git`, `:github review` is read-only and
`:github ship TITLE` is confirmation-gated.

If Chrome is already running or its preferred CDP port is occupied, Glass
keeps the TUI alive and shows attach, automatic-port, and retry choices. The
recovery card explains whether the endpoint was verified before any attach
action is queued.

For a read-only inventory outside the TUI, `glass archive-targets` prints a
bounded redacted archive; `--output targets.json` writes it to a
policy-approved path. It never selects, closes, or captures page content.

From the App surface, chat can bootstrap a browser target with `open <url>`.
Glass attaches, navigates, and observes through the revision-bound browser
workspace before the agent acts.

## Source and diff rendering

The Code surface classifies both source files and diff hunks by path. It uses
the bundled `syntect` grammar first, then deterministic manual highlighting;
unknown formats deliberately use a plain-text fallback. Path aliases cover
TypeScript, Swift, Kotlin, Dart, and Dockerfile-like names. Markdown headings
and inline markup are styled directly, fenced code tracks its declared
language (including aliases), and recognized Mermaid flowcharts and sequence
diagrams receive a terminal-native preview. If Mermaid syntax is not
recognized, its source remains readable rather than being dropped.

The native editor keeps cursor and selection state in the shared workspace, including unsaved buffers. `Alt-A` sends the focused buffer, cursor, and selection to the in-TUI Pi conversation without writing a file. The Code surface's `REVIEW` panel summarizes anchored comments, pending proposals, and checkpoints; use the `:editor` routes to add or resolve comments, create or approve/reject proposals, and create or restore checkpoints. Proposals are exact-content and conflict-checked; checkpoints and proposals change resident buffers only until `:editor save` or `Ctrl-S`.


## Live browser and iPhone


`live on` enables an ephemeral terminal-native browser using Herdr-owned
graphics when the native pane is available, Kitty terminal graphics when
`--tui-live-backend kitty` is selected, or the bounded true-color ANSI
half-block renderer with the default/ANSI backend. `live auto` enables
continuous frames when the selected backend is available and may remain
semantic-only with the default backend when Herdr is absent. An explicit
unavailable Herdr backend remains semantic-only. `live quality data` is
intended for constrained links. Herdr is optional but recommended as the
persistent PTY owner for detach/reattach. tmux remains compatible.

The active BrowserSession's private iPhone path is exposed by the development
TUI's `:browser remote-open` route. It starts a tokenized, loopback-only,
revocable view and prints its SSH-forward hint. `browser remote-view open` is
not a standalone CLI command (and the focused `glass-browser` package does not
export the development runtime); configure the matching SSH local port forward,
then open the printed local URL in Safari.

Do not expose the application server, Remote View, or Chrome CDP publicly.

## MCP and clients

```console
glass mcp-config --client generic
glass --mcp
```

MCP uses newline or `Content-Length` JSON-RPC over stdio. stdout is protocol-
only; diagnostics use stderr. Browser and project tools share one negotiated,
policy-aware server lifecycle. Resident project PTYs remain owned until
explicit detach, eviction, server shutdown, or daemon shutdown.

Repository TypeScript and Python clients provide typed browser/project helpers,
cursor-bounded event subscriptions, cancellation, process-health waits,
reconnect workflows, and mutation-lease scopes. They are not published to npm
or PyPI in the `0.3.12` line.

## Agents

The deterministic local harness supports hello, prompt, steering, and bounded
tool calls. The native Pi SDK runtime adds model selection, thinking level,
follow-up, abort, persistent sessions, resume/fork/compact, and event streaming.
Pi starts with its built-in tools disabled; its only executable capability is
Glass's schema-validating gateway into the authoritative workspace actor.

The same workspace exposes the fixed external-harness catalog to both clients:
`glass harness list` and `:harness list` report PATH availability, while
`glass harness start NAME --root .` and `:harness start NAME` launch the same
interactive handoff.

It can also make a temporary one-shot delegation to installed Codex CLI, Claude
Code, or OpenCode through `glass agent delegate HARNESS PROMPT --root .` or
the TUI's `:harness delegate NAME PROMPT` route. Delegation is read-only by
default, returns bounded structured output, and requires explicit mutation
authority plus confirmation for a `workspace-write` sandbox.

Run `glass agent doctor` or `glass agent status` before the first turn. Nothing
is downloaded at startup. `glass agent setup` explicitly installs or repairs
the exact SDK version pinned by this release; `glass agent setup --update`
forces a reinstall of that pinned version, while `--sdk-entry` selects an
existing SDK. `glass agent setup --login` opens Pi's provider login flow; Glass
reports credential presence/expiry without printing secrets.

Prompt text, authored task values, and tool arguments are not stored in raw
audit events. Mutating tools require authority and explicit confirmation.

Launch `glass --yolo` only for a fully trusted local development session. It
turns off Pi/Glass tool confirmation, automatically accepts extension
confirmation RPCs, grants browser policy capabilities without confirmation,
and loads installed Pi resources and their tools. The TUI displays a persistent
`YOLO` marker. Revision guards, daemon/workspace leases, explicit host denials,
and transport/result bounds remain enforced.

The resident Pi SDK session uses a Glass-specific system prompt and one
Glass-owned gateway into project, runtime, diagnostic, Web IR, task, browser,
and workflow tools. It streams completed message and tool events through the Agent view while dropping token-level redraw noise;
steer and abort remain responsive during a running turn. If a turn is
cancelled or the worker fails, resubmitting from the composer restarts the
selected interactive session instead of requiring a new CLI session. Unknown
tool names are rejected once and the current turn is aborted. Ambient Pi
extensions, skills, context files, and themes are disabled for deterministic
local-first behavior. Every mutation pauses on a Glass-owned confirmation
sheet. `Y` or Enter grants one use for the already serialized call; `N` or
Esc denies it.
Requests expire after 120 seconds, concurrent requests fail closed, and the
one-shot/non-interactive path always denies UI requests. Exact preconditions,
workspace confinement, atomic saves, bounded execution, actor attribution,
revisions, and leases remain enforced by Glass.

The standard names keep useful coding semantics: line-paged reads; bounded,
path-filtered listings; literal UTF-8 grep with glob/case/context controls;
`*`/`?` path finding; atomic exact-match edits; and commands with an explicit
300-second ceiling. Files that are non-UTF-8 or over the project-file bound are
skipped by grep rather than copied into the model context.

All Pi built-in and `models.json` providers remain selectable. Ambient Pi
resources stay off by default; trusted users can opt into live catalog refresh,
session persistence, or installed context/extensions/skills with
`GLASS_PI_ONLINE_CATALOG=1`, `GLASS_PI_PERSIST_SESSION=1`, and
`GLASS_PI_TRUSTED_RESOURCES=1` respectively. Trusted-resource mode also removes
the extension-tool allowlist, so tools registered by installed extensions are
available; those tools execute outside Glass's broker and approval boundary.

## Browser usage

```console
glass --incognito navigate https://example.com
glass observe --level interactive
glass click r7:b42 --expected-revision 7
```

Observation is structured-first. Screenshots, full DOM, form values, PDFs, and
JavaScript evaluation are explicit and policy-sensitive.

## Documentation

- [Getting started](https://github.com/wanazhar/glass/blob/main/docs/getting-started.md)
- [Complete feature reference](https://github.com/wanazhar/glass/blob/main/docs/features.md)
- [Development Runtime](https://github.com/wanazhar/glass/blob/main/docs/development-runtime.md)
- [Mobile and remote development](https://github.com/wanazhar/glass/blob/main/docs/mobile-remote.md)
- [CLI reference](https://github.com/wanazhar/glass/blob/main/docs/cli.md)
- [MCP integration](https://github.com/wanazhar/glass/blob/main/docs/mcp.md)
- [Complete MCP tool catalog](https://github.com/wanazhar/glass/blob/main/docs/mcp-tools.md)
- [Security policy](https://github.com/wanazhar/glass/blob/main/SECURITY.md)
- [Complete uninstall and retained state](https://github.com/wanazhar/glass/blob/main/docs/installation.md#fully-uninstall-glass)

License: MIT.
