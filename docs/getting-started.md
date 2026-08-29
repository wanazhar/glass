# Getting started

This guide takes you from installation to one verified Glass workflow. Choose
the path that matches your goal. All paths use local processes and local state;
Glass does not provide a hosted browser, code service, or autonomous planner.

**Status: Current 0.3.14 source behavior.** This guide follows the checked-in
source checkout; checkout-only TUI behavior is labeled as current source, not
as an immutable published-release guarantee.


## Before you begin

You need stable Rust 1.88 or newer for a source installation. Browser-backed
paths also need Chrome or Chromium. Project inspection, Task Protocol, Web IR,
policy, capability, and most development-runtime operations are browser-free.

Check the current support and release boundary before deployment:

- [Installation and platform requirements](installation.md)
- [Feature parity](feature-parity.md)
- [Recorded platform evidence](local-platform.md)

## Select the product

| Package | Installs | Use it for |
|---|---|---|
| `glass-dev` | `glass`, `glass-browser` | Complete CLI/TUI/MCP product, project runtime, agents, remote cockpit, and browser control |
| `glass-browser` | `glass-browser` and Rust crate `glass_browser` | Standalone browser control plane and embeddable Rust API |

Install the complete environment:

```console
cargo install glass-dev --locked
glass doctor
```

Install only the browser package:

```console
cargo install glass-browser --locked
glass-browser doctor
```

After installation, preview the resolved owner and root before updating:

```console
glass update --dry-run
glass update
# Use this form when only the core command is installed:
glass-browser update
```

The command updates the existing Cargo package; it does not switch products.
Read the [complete update contract](installation.md#update-a-cargo-installation)
for custom registries, pinned versions, unmanaged builds, and recovery.

`doctor` does not start Chrome. It reports executable and browser discovery,
platform status, policy, profiles, daemon health, stores, and remediation. A
`degraded` result identifies each missing dependency instead of partially
starting a session.

## Prepare Pi directly (optional)

The direct development-product commands are useful before opening the TUI:

```console
glass agent doctor
glass agent status
glass agent setup
glass agent setup --login
glass agent setup --update
```

`setup` installs or repairs the pinned managed SDK; `--login` opens Pi's
provider login flow in the terminal, and `--update` forces a reinstall of that
pinned version. Nothing is downloaded merely by starting Glass. The focused
`glass-browser` package has no Pi or development-agent routes.

The two packages intentionally compete for the `glass-browser` command in one
Cargo root. Use the [package transition](installation.md#install-from-source)
and [complete uninstall](installation.md#fully-uninstall-glass) procedures when
switching products.

## Path A: inspect and run a project

Use this path when you want Glass as a local development workspace without a
browser.

1. Enter the project and inspect detection:

   ```console
   cd /path/to/project
   glass project inspect --root .
   ```

   The result identifies the canonical root, detected ecosystem, configured
   commands, and optional browser URL. Glass reads `glass.toml` or
   `.glass.toml` when present. It does not execute a detected command during
   inspection.

2. Inspect the bounded project tree and, when using the resident TUI, start
   the language server before requesting diagnostics:

   ```console
   glass project files --root .
   ```

   In the TUI command palette, enter:

   ```text
   lsp start rust-analyzer rust-analyzer
   lsp diagnostics rust-analyzer src/main.rs
   ```

   The one-shot convenience command starts its dedicated Rust client as
   needed:

   ```console
   glass project diagnostics src/main.rs --root .
   ```

   The tree reports its entry limit, truncation, ignored generated/vendor
   directories, and skipped symlinks. Missing language-server executables and
   protocol failures are explicit errors; Glass does not fabricate diagnostics.

3. Run a finite command in a real PTY:

   ```console
   glass project run check --command "cargo check" --wait --root .
   ```

   `--wait` keeps the one-shot CLI alive until the child exits. For a
   long-running server, use the TUI or MCP project session so one resident
   owner can accept input, resize, restart, stop, and retain bounded output.

4. Review attributed state:

   ```console
   glass project diff --root .
   glass project timeline --root .
   glass project replay --root .
   ```

Project reads, writes, renames, and deletes remain inside the canonical root.
Saves are atomic. An external file change or conflicting actor claim produces
a conflict instead of overwriting another version.

## Path B: use the terminal workspace

Start the resident workspace from a project:

```console
cd /path/to/project
glass
```

The complete first-run path is:

```text
glass → Trust (if required) → Agent → Pi setup/login → composer
       └─ I inspect · O open · 1 trust once · T trust project
```

An untrusted repository opens `Trust`; choose `I`, `O`, `1`, or `T` as shown
in the footer. Until a local choice, repository-controlled execution remains
blocked. On `Agent`, type a prompt or press `Enter`. If Pi is not ready,
`:agent setup` queues installation/repair of the pinned runtime; approve with
`Enter`/`Y` or cancel with `Esc`/`N`. Run `:agent setup login` to hand the
terminal to Pi `/login` and exit Pi to return. `:agent update` refreshes the
pinned runtime; `:agent doctor` and `:agent status` report readiness.

Talk from any surface with `Ctrl-L`. Default composer mode is Agent.
`Ctrl-Shift-A` cycles Ask, Plan, and Agent; Ask and Plan do not mutate. When
ready, `Enter` sends the draft and leaves the dock open for the next prompt;
`Esc` closes it. `Ctrl-D` toggles steer mode; default follow-up mode queues the
next message. Sent prompts stay as `YOU`, and `GLASS AGENT` streams the reply
and tool activity. A failed send restores the draft for editing and retry;
background work retains newly typed text. Mutating calls pause for one-use
approval (`Enter`/`Y` approve, `Esc`/`N` deny).

Desktop uses `1`–`8` for Agent, Code, App, Terminal, Tasks, Git, Debug, and
More. Phone uses `1`–`5` for Agent, Code, App, Tasks, and More. Auto chooses
phone below 72 columns or 22 rows, compact below 118 columns or 32 rows, and
desktop otherwise; override with `--tui-layout mobile|compact|desktop`.
`Tab`/`Shift-Tab` cycle surfaces. Type in the dock to talk, `:` to search
commands, `a` to open this surface's actions, and `?` to open help.

Use these first routes:

```text
:process start dev                 # detected dev suite, confirmation-gated
:task list                         # inspect task state
:cockpit start                     # More: private loopback cockpit
:browser start                     # headed, persistent browser
:browser targets                   # search and select a page target
```

On Code, select a file and press `Enter` (or `i`) to enter the full-screen
editor. The read-only preview wraps long lines on narrow terminals while
preserving syntax highlighting. In the editor, `Alt-W` toggles soft wrap
(off by default); on, lines wrap at whitespace where possible with continuation
gutters and synchronized cursor/selection/highlighting. Off horizontally
scrolls source columns. `Ctrl-S` saves, `Ctrl-Z`/`Ctrl-Y` undo/redo, and
`Alt-A` asks Pi with focused path/cursor/selection and unsaved content attached,
with an explicit do-not-edit request. The editor starts in INSERT. `Esc`
returns to NORMAL. `Esc` from NORMAL on a clean buffer leaves the editor.
Unsaved buffers still ask: `S` save, `D` discard, `Q` discard-and-quit, or
`Esc`/`N` stay. `Ctrl-C` opens Glass quit confirmation from editor input; if
the unsaved-exit prompt is already open, its save/discard/stay choices take
priority.

The Code `REVIEW` panel exposes anchored comments, proposals, and checkpoints.
Use `:editor comment-selection TEXT`, `:editor comment PATH START END TEXT`,
`:editor comment-resolve ID`, `:editor propose PATH SUMMARY TEXT`, `:editor
proposals`, `:editor accept ID`, `:editor reject ID`, `:editor checkpoint NAME`,
`:editor restore CHECKPOINT_ID`, and `:editor replace-selection TEXT`.
Proposals are exact-base and become stale on conflicting edits; resident
buffers change only until `:editor save PATH` or `Ctrl-S`.

The resident process owns editor buffers, PTYs, language services, browser,
agent, and event state. Browser recovery leaves project, process, editor, and
agent state alive.

Continuous browser pixels are off by default. In the development TUI use
`:browser view` to toggle the selected presentation backend; use
`--tui-live ...` before launch to choose it. The standalone browser TUI uses
`live on`/`live off`.

## Path C: observe and act in a browser

Use a dedicated profile or incognito session. Keep CDP on loopback.

For disposable state:

```console
glass --incognito --headed navigate https://example.com
```

For an authenticated profile:

```console
glass profiles create work
glass --profile work --headed navigate https://example.com
```

In a resident TUI, MCP, daemon, or Rust session, observe before acting:

```console
glass observe --level interactive
```

The result includes a browser revision and bounded references such as
`r7:b42`. Use both for the action:

```console
glass click r7:b42 --expected-revision 7
```

The action path is resolve exactly one target, check revision and policy,
dispatch, observe again, and report verification. Glass refuses ambiguous and
stale targets. If transport fails after dispatch, the result can be
`indeterminate`; re-observe or use returned recovery data instead of retrying
blindly.

Normal observation does not request screenshots, full DOM, form values,
cookies, PDFs, network diagnostics, or evaluated JavaScript. Invoke those
operations explicitly and treat their output as sensitive.

Continue with [Semantic observations](semantic-observation.md), [Actions and
revisions](actions.md), and [Policy](policy.md).

## Path D: use Glass from an iPhone or remote shell

Connect to the machine that owns the project and credentials, then start the
mobile layout:

```console
ssh you@workstation
cd /path/to/project
glass --tui-layout mobile
```

Glass separates terminal geometry, network transport, graphics capability,
shell, and multiplexer. A phone-sized terminal does not by itself lower local
render policy, and an SSH environment variable does not prove graphics
support.

The semantic-first phone UI works without pixels. Use Herdr or tmux to retain
the PTY across detach. In the development TUI, use `:browser view` for the
selected live backend. For the private iPhone browser, run `:browser
remote-open`; Glass prints a tokenized loopback URL and a hint equivalent to:

```console
ssh -N -L PORT:127.0.0.1:PORT USER@HOST
```

Run that forward from the iPhone-side network, then open the printed local URL
in Safari. Use `:browser remote-status` while sharing and `:browser
remote-revoke` when finished. Never expose Chrome CDP publicly.

Follow [Mobile and remote development](mobile-remote.md) for forwarding,
Herdr, Mosh, Remote View, browser recovery, terminal compatibility, and
troubleshooting.

## Path E: connect an MCP client

Generate configuration with an absolute executable path:

```console
glass mcp-config --client generic
glass mcp-config --client claude-code
glass mcp-config --client codex
```

The client must:

1. start `glass --mcp` with stdout reserved for protocol frames;
2. send `initialize` and accept the returned version/capability contract;
3. send `notifications/initialized`;
4. inspect `tools/list` for the exact installed schemas; and
5. cancel bounded waits when the calling task is abandoned.

Browser and project tools share the resident server's state. Mutating daemon
operations additionally require the current mutation lease. Read [MCP
integration](mcp.md) before writing a client.

## Path F: embed the Rust library

Add the focused package:

```toml
[dependencies]
glass = { package = "glass-browser", version = "0.3" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

The crate name is `glass_browser` unless Cargo renames it. The embedding
application owns `BrowserSession` lifecycle. Call `close().await` for owned
sessions so Chrome can flush profile state; dropping an attached session must
not close the external browser.

Read the [Rust SDK guide](rust-sdk.md) for launch/attach, policy, observations,
actions, semantic execution, workflows, knowledge, backends, presentation,
errors, and optional development-runtime APIs.

## Common first-run failures

| Symptom | Meaning | Recovery |
|---|---|---|
| `doctor` cannot find Chrome | No supported executable was discovered | Install system Chrome/Chromium, run `install-chromium` where supported, or pass `--chrome-path`. |
| Chrome exits with profile status 21 | A confined browser cannot access a host profile path | Upgrade to 0.3.4 or newer; Glass automatically selects Snap Chromium's accessible persistent profile root. |
| Pi not ready or setup/login fails | Node, the pinned SDK, or provider auth is unavailable | Run `glass agent doctor`, retry `glass agent setup`/`glass agent setup --login`, or update with `glass agent setup --update`; the TUI remains available. |
| Live backend unavailable or capture fails | Requested native graphics path or screenshot was not usable | Use `:browser view` with semantic-only state, choose explicit ANSI, or turn live mode off; semantic observation remains available. |
| Port `9222` is occupied | Another process owns the preferred CDP port | In the TUI run `browser start`; the recovery sheet can attach to a verified endpoint, launch an isolated browser on a free local port, or retry `9222`. Project and agent state remain alive. |
| Multiple targets found | More than one page is eligible | Run `browser targets` (optionally followed by a query), then select the intended target from the App surface. |
| LSP unavailable | The detected language server is missing or failed initialization | Install the server, inspect diagnostics, and retry; Glass does not fabricate diagnostics. |
| One-shot process cannot remain running | The CLI owner is exiting | Use `--wait` for finite work or use TUI/MCP/daemon for a resident process. |
| Stale revision | Browser/project state changed after observation | Re-observe or reread, then issue a new revision-bound request. |
| MCP request rejected before tools | Initialization lifecycle is incomplete | Complete negotiation and send `notifications/initialized`. |

## Close safely

- Exit the TUI cleanly so it restores terminal state and writes the bounded
  reconnect capsule.
- Stop owned project processes before deleting their project or uninstalling
  Glass.
- Stop the daemon with `glass daemon stop` before uninstalling `glass-dev`.
- Close owned browser sessions so named profiles can flush.
- Revoke Remote View with `browser remote-revoke` when sharing ends.
- Do not assume an interrupted mutation is safe to repeat; reconcile its
  execution ID and current revision.

## Next steps

- [Complete feature reference](features.md)
- [CLI reference](cli.md)
- [Development Runtime](development-runtime.md)
- [Mobile and remote development](mobile-remote.md)
- [MCP integration](mcp.md)
- [Rust SDK](rust-sdk.md)
- [Security policy](../SECURITY.md)
