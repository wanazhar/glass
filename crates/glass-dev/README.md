# glass-dev

`glass-dev` installs `glass` and `glass-browser`, the full terminal-native Glass environment. It
combines the browser control plane from `glass-browser` with a bounded project
runtime, Ratatui development workspace, local agent harness, MCP server, and
phone-oriented remote cockpit.

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

Chrome or Chromium is needed only for browser-backed operations. Project
inspection, files, search, local harness, Task Protocol/Web IR operations,
policy preflight, and many diagnostics are browser-free.

## Terminal workspace

```console
cd /path/to/project
glass
```

Desktop mode exposes browser and Development workspaces. The Development view
owns a bounded file tree, native editor, PTY processes, diagnostics, actors,
timeline, source/runtime graph, diff, replay, and optional Pi/Neovim adapters.

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

First-launch agent setup stays inside the TUI. On the Agent surface, press `s`
to install or repair the pinned managed Pi runtime, `u` to refresh that pinned
runtime, `l` to open Pi `/login`, and `i` to start a conversation. The
equivalent palette routes are `:agent setup` and `:agent update`. `a` opens the
guided command center; `?` opens contextual help. Agent mutations pause on an
inline Glass approval card: `Enter` approves the exact call once and `Esc`
denies it.

On the App surface, `T` opens a searchable page-target picker without
changing the active page. Type a title, URL, or target ID; `j`/`k` (or the
arrow keys) select a result and `Enter` queues an explicit, one-use target
selection. `t` remains the shortcut for typing into the selected page.

The Terminal surface is the dev-suite entrypoint: `s` starts the detected
project command behind the same confirmation card, while `a` exposes process
start, logs, input, and health actions. Browser start from the TUI is headed
and persistent by default so existing authenticated profiles remain usable;
use `:browser start --incognito --headless` for a disposable automation
session.

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

`live on` enables an ephemeral terminal-native browser using Herdr-owned
graphics when the native pane is available, or the bounded true-color ANSI
half-block renderer. `--tui-live-backend kitty` is accepted for compatibility
and currently uses the ANSI path; `live quality data` is intended for
constrained links. Herdr is optional but recommended as the persistent PTY
owner for detach/reattach. tmux remains compatible.

The `safari` command prints the stable application-server iPhone workflow. For
the active BrowserSession, `browser remote-view open` starts a tokenized,
loopback-only, revocable view and prints its SSH-forward hint. Configure the
matching SSH local forward, then open the printed local URL in Safari. Do not
expose the application server, Remote View, or Chrome CDP publicly.

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
or PyPI in the `0.3.11` line.

## Agents

The deterministic local harness supports hello, prompt, steering, and bounded
tool calls. The native Pi SDK runtime adds model selection, thinking level,
follow-up, abort, persistent sessions, resume/fork/compact, and event streaming.
Pi starts with its built-in tools disabled; its only executable capability is
Glass's schema-validating gateway into the authoritative workspace actor.

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
