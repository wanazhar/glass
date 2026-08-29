# glass-dev

`glass-dev` installs `glass` and `glass-browser`, the full terminal-native Glass
coding environment. The coding workspace is the primary surface: it combines a
bounded project runtime, Ratatui editor, local agent harness, MCP server, PTYs,
tests, diagnostics, Git diff, and a phone-oriented remote cockpit. The browser
control plane is an integrated optional app surface for UI work and verification.

**Status: Current 0.3.13 source behavior.** This page documents the checked-in
source checkout; checkout-only TUI behavior is labeled here rather than
presented as published-release behavior.


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
for installed command behavior. This guide follows the current `0.3.13` source
checkout. Published docs.rs pages match the crate version they were built from
(`0.3.13` at last publication). Checkout-only TUI behavior can be newer than
that published API page.


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

```text
Desktop: navigation | active surface / editor / browser | context
          status and command/composer footer
Phone:   header
         one active surface: Agent | Code | App | Tasks | More
         status, command palette, or composer
```

Terminal geometry selects Auto layout: phone below 72 columns or 22 rows,
compact below 118 columns or 32 rows, desktop otherwise. Force one with
`--tui-layout mobile|compact|desktop`; geometry does not infer network or
graphics capability. Start from a project and inspect it without opening
Chrome:

```console
cd /path/to/project
glass
glass project inspect --root .
glass project files --root .
glass project run check --command "cargo check" --wait --root .
glass project diagnostics src/main.rs --root .
glass project diff --root .
glass agent prompt "read README.md" --root .
```

On first run, an untrusted project opens the Trust surface: `I` inspects, `O`
opens untrusted, `1` trusts once, and `T` trusts the project. Repository
execution remains blocked until a local decision. On Agent, type a prompt or
press `Enter`; if Pi is unready, use `:agent setup`, approve its one-use
confirmation with `Enter`/`Y`, then use `:agent setup login` for Pi `/login`.
`:agent update` refreshes the pinned runtime; `:agent doctor` and `:agent
status` report readiness. A setup/login failure leaves the workspace available
for retry.

Talk from any surface. `Ctrl-L` opens the shared composer dock (not a redraw).
`Alt-A` docks from the editor. Default mode is Agent. `Ctrl-Shift-A` cycles
Ask, Plan, and Agent; `/ask`, `/plan`, `/agent`, and `/todo` also set the mode.
Ask and Plan are fail-closed for mutations. `Enter` sends and keeps the dock
open; `Esc` closes it. `Ctrl-D` toggles steer mode; follow-up mode queues a
message. Sent prompts remain as `YOU`, and `GLASS AGENT` streams replies and
tool activity. A failed send restores the draft for edit-and-retry; busy
background work retains new text. Mutations pause on a Glass approval card
(`Enter`/`Y` approve once, `Esc`/`N` deny). Type in the dock to talk, `:` to
search commands, and `a` to open this surface's actions. `:process start dev`
starts the detected suite, Tasks owns the workspace-local Agent checklist
(`.glass/todos/session.json`) plus the overnight DAG, and `:cockpit start` in
More opens a tokenized loopback-only cockpit. Enter on
More `doctor` stays on More and updates the PI panel.

On the App surface, `:browser targets` opens a searchable picker; type a
redacted title, URL, or target ID, use arrows, and press `Enter` to queue one
explicit selection. `:browser start` is headed/persistent by default;
`:browser start --incognito --headless` is disposable. Startup collisions keep
the TUI alive and offer attach, free-port, retry, or dismiss recovery.

The resident process owns editor buffers, PTYs, language services, browser,
agent, and timeline state. Browser recovery replaces only browser state.

Project paths remain inside the canonical root. Reads, buffers, PTY output,
events, search results, and retained sessions are bounded. Saves are atomic and
actor-attributed; external changes and conflicting edit claims fail closed.

## Source and diff rendering

The Code surface classifies both source files and diff hunks by path. It uses
the bundled `syntect` grammar first, then deterministic manual highlighting;
unknown formats deliberately use a plain-text fallback. Path aliases cover
TypeScript, Swift, Kotlin, Dart, and Dockerfile-like names. Markdown headings,
inline markup, fenced languages, and recognized Mermaid diagrams render
terminal-natively.

The file list and Code preview are read-only until full-screen edit. Narrow
terminals wrap long preview lines to the pane width while keeping highlighting.
Select a file and press `Enter` (or `i`) to edit. `Alt-W` toggles soft wrap,
off by default; on, lines wrap at whitespace where possible, continuation rows
retain gutter alignment, and cursor/selection/highlighting stay synchronized.
Off uses horizontal source-column scrolling. Arrows move, `Shift`+arrows
select, `Ctrl-S` saves, `Ctrl-Z`/`Ctrl-Y` undo/redo, and `Alt-A` sends the
focused path, cursor, selection, and unsaved content to Pi with a do-not-edit
prompt. The editor starts in INSERT. `Esc` returns to NORMAL. `Esc` from
NORMAL on a clean buffer leaves the editor. Unsaved work still asks: `S` save,
`D` discard, `Q` discard and quit, or `Esc`/`N` stay. `Ctrl-C` opens Glass
quit confirmation from editor input; an already-open unsaved-exit prompt keeps
its save/discard/stay choices.

The native editor also provides modal motions/operators, tree-sitter
textobjects with lexical fallback, FIM ghosts, LSP navigation/inlays, gutter
evidence marks, and bounded proposal hunk review.

The `REVIEW` panel shows open comments, pending proposals, and checkpoints.
Use `:editor comment-selection TEXT`, `:editor comment PATH START END TEXT`,
`:editor comment-resolve ID`, `:editor propose PATH SUMMARY TEXT`, `:editor
proposals`, `:editor accept ID`, `:editor reject ID`, `:editor checkpoint
NAME`, `:editor restore CHECKPOINT_ID`, and `:editor replace-selection TEXT`.
Proposals are exact-base and conflict-checked; stale proposals are not
applied. Checkpoints/proposals affect resident buffers until `:editor save PATH`
or `Ctrl-S`.
`--tui-live off` is the default. With `--tui-live auto` and backend `auto`,
Herdr is used only when detected; otherwise presentation stays semantic-only.
`--tui-live on` with backend `auto` uses the bounded ANSI fallback. Explicit
`--tui-live-backend kitty` emits Kitty terminal graphics; explicit `ansi`
renders true-color Unicode half-blocks; an unavailable explicit Herdr backend
remains semantic-only. Quality `data`, `balanced`, and `smooth` targets
approximately 3, 6, and 12 FPS. In the development TUI, `:browser view`
toggles the selected path; the standalone `glass-browser` TUI instead uses
`live on`/`live off`.


Herdr is optional but can own a persistent PTY for detach/reattach; tmux and
Mosh remain compatible for terminal transport. The development TUI's
`:browser remote-open` starts a tokenized, revocable, loopback-only Remote
View and prints an SSH-forward hint; configure the matching local forward and
open the printed URL in Safari. The standalone `glass-browser` package does
not provide Remote View or the project/agent runtime. Never expose CDP,
Remote View, or the application server publicly.


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
or PyPI in the `0.3.13` line.

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
the exact SDK version pinned by this checkout; `glass agent setup --update`
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
