# glass-dev

`glass-dev` installs `glass` and `glass-browser`, the full terminal-native Glass
coding environment. The coding workspace is the primary surface: it combines a
bounded project runtime, Ratatui editor, local agent harness, MCP server, PTYs,
tests, diagnostics, Git diff, and a phone-oriented remote cockpit. The browser
control plane is an integrated optional app surface for UI work and verification.

**Status: Current source checkout.** The workspace package version is `0.3.14`;
this checkout includes changes after the published `0.3.14` release. See the
[release evidence](https://github.com/wanazhar/glass/blob/main/docs/release-evidence.md)
for the published source and artifacts. Checkout-only TUI behavior is labeled
here rather than presented as published-release behavior.


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
for installed command behavior. This guide follows the current `0.3.14` source
checkout. Published docs.rs pages match the crate version they were built from
(`0.3.14` at last publication). Checkout-only TUI behavior can be newer than
that published API page.


Chrome or Chromium is needed only for browser-backed operations. Project
inspection, files, search, local harness, Task Protocol/Web IR operations,
policy preflight, and many diagnostics are browser-free.

## Terminal workspace

```console
cd /path/to/project
glass
```

Desktop mode opens the Development coding workspace by default. Its five
primary destinations are Agent, Code, Browser, Terminal, and Git. Tasks, Debug,
and services/settings remain available as contextual `/` actions. The
workspace owns a bounded file tree, native editor, PTY processes, diagnostics,
actors, timeline, source/runtime graph, diff, replay, and optional Neovim
adapters.

```text
Desktop: navigation | active surface / editor / browser | context
          status and command/composer footer
Phone:   header
         one active surface: Agent | Code | Browser | Terminal | Git
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

Glass opens directly on Agent with the composer ready; type a prompt
immediately. When the resident engine is not ready, Glass reports the exact
repair step — install `glass-core`, set `GLASS_CORE_BINARY`, or connect a
provider. Press `/` and filter for Readiness doctor or Glass session status to
inspect readiness. Provider setup stays in Glass
Core, and Glass reports readiness without printing credentials.

Talk from any surface. `Ctrl-L` opens the shared composer dock (not a redraw).
`Ctrl-Shift-T` opens the project conversation picker. Glass Core owns provider
connections, agent execution, and native coding-tool permissions. Glass owns
workspace, browser, Git, and review integrations. It executes Core-authorized
tool calls after checking the current workspace and resource revisions; it
does not ask for a second approval. Direct human actions retain their explicit
UI behavior. Ordinary Agent messages do not attach a file or selection. Opening
the composer from Code shows a removable context chip for the viewed buffer;
selected text takes priority, unsaved content is labeled, and `Ctrl-Shift-X`
removes or restores the bounded context before send.
`Enter` sends and keeps the dock open; `Esc` closes it. `Ctrl-D` toggles steer
mode; follow-up mode queues a message. `Ctrl-Shift-A` opens Attention Inbox.
Sent prompts remain as `YOU`, and `GLASS AGENT` streams replies and tool
activity. A local or pre-dispatch send failure restores the draft for
edit-and-retry; busy background work retains new text. If Core returns an error
for a correlated `turn/prompt` or `turn/followUp`, Glass marks the attempt
failed and returns its agent phase to idle. The failure is summarized with a
safe error class and code, then listed in Attention Inbox. Inspect the owning
conversation and choose Retry failed or uncertain message from `/` to request
explicit confirmation before a possibly uncertain prompt is sent again; Glass never replays the attempt
automatically. Type in the Agent dock to talk; `/` discovers the live Core
command catalogue and Glass workspace actions without sending a prompt. Core
runtime commands stay with Core, interactive commands hand off to Core's native
UI, and Glass actions operate workspace surfaces. `Ctrl-K` opens the general
Glass command palette. Press `a` on
non-Agent surfaces for their actions. `:process start dev`
starts the detected suite, Tasks owns the workspace-local Agent checklist
(`.glass/todos/session.json`) plus the overnight DAG. Tasks, Debug, and
workspace services are advanced destinations available through the `/` Glass
action group; they no longer occupy persistent primary tabs. Services and
Experiments remain live projections, while route actions run only after an
explicit selection.

On the Browser surface, `Ctrl-K` opens browser actions; from Agent, filter the
`/` catalogue by `glass browser`. `browser targets` opens a searchable picker; type a
redacted title, URL, or target ID, use arrows, and press `Enter` to queue one
explicit selection. `browser start` from `Ctrl-K` is headed/persistent by
default; `browser start --incognito --headless` is disposable. Startup collisions keep
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
focused path, cursor, selection, and unsaved content to the resident agent with a do-not-edit
prompt. The editor starts in INSERT. `Esc` returns to NORMAL. `Esc` from
NORMAL on a clean buffer leaves the editor. Unsaved work still asks: `S` save,
`D` discard, `Q` discard and quit, or `Esc`/`N` stay. `Ctrl-C` opens Glass
quit confirmation from editor input; an already-open unsaved-exit prompt keeps
its save/discard/stay choices.

The native editor also provides modal motions/operators, tree-sitter
textobjects with lexical fallback, FIM ghosts, LSP navigation/inlays, gutter
evidence marks, and bounded proposal hunk review.

The `REVIEW` panel shows open comments, pending proposals, and checkpoints.
Use `Ctrl-K` and the `editor` routes for `comment-selection TEXT`,
`comment PATH START END TEXT`, `comment-resolve ID`, `propose PATH SUMMARY
TEXT`, `proposals`, `accept ID`, `reject ID`, `checkpoint NAME`,
`restore CHECKPOINT_ID`, and `replace-selection TEXT`.
Proposals are exact-base and conflict-checked; stale proposals are not
applied. Checkpoints/proposals affect resident buffers until `editor save PATH`
from the palette or `Ctrl-S`.
`--tui-live off` is the default. With `--tui-live auto` and backend `auto`,
Herdr is used only when detected; otherwise presentation stays semantic-only.
`--tui-live on` with backend `auto` uses the bounded ANSI fallback. Explicit
`--tui-live-backend kitty` emits Kitty terminal graphics; explicit `ansi`
renders true-color Unicode half-blocks; an unavailable explicit Herdr backend
remains semantic-only. Quality `data`, `balanced`, and `smooth` targets
approximately 3, 6, and 12 FPS. In the development TUI, `browser view` from
`Ctrl-K` toggles the selected path; the standalone `glass-browser` TUI instead uses
`live on`/`live off`.


Herdr is optional but can own a persistent PTY for detach/reattach; tmux and
Mosh remain compatible for terminal transport. The development TUI's
`browser remote-open` action from `Ctrl-K` starts a tokenized, revocable, loopback-only Remote
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
or PyPI in the `0.3.14` line.

## Agents

The deterministic local harness supports hello, prompt, steering, and bounded
tool calls. Resident agents use Glass Core by default. Glass starts
`glass-core runtime`, registers the session's tool catalog, and executes each
tool call in the workspace. Glass Core provides model selection, thinking
levels, follow-up messages, abort, persistent sessions, resume, fork, compact,
and event streaming. It starts without built-in tools and is the sole resident
agent engine.

The same workspace exposes an informational external-harness catalog to the
CLI and the Glass `/` actions. `glass harness list` and `/` → Glass → Harness
catalog report PATH availability. `glass harness start NAME --root .` remains
a CLI-only human handoff; the Glass workspace does not dispatch external coding
agents because Core owns agent execution.

The CLI retains an explicit one-shot delegation command for installed Codex
CLI, Claude Code, or OpenCode. Glass does not expose external-agent delegation
from its TUI; send agent work to the resident Core session.

Run `glass agent doctor` or `glass agent status` to check Glass Core. Glass
does not download an engine when it starts. Provider setup and credentials are
managed by Glass Core; readiness output never prints credential values.

Prompt text, authored task values, and tool arguments are not stored in raw
audit events. Mutating tools require authority and explicit confirmation.

Glass Core owns native coding permissions. Glass records workspace activity
and applies revision and confirmation rules to browser, Git, and other Glass
integrations.

The standard names keep useful coding semantics: line-paged reads; bounded,
path-filtered listings; literal UTF-8 grep with glob/case/context controls;
`*`/`?` path finding; atomic exact-match edits; and commands with an explicit
300-second ceiling. Files that are non-UTF-8 or over the project-file bound are
skipped by grep rather than copied into the model context.


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
