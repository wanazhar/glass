# Getting started

This guide takes you from installation to one verified Glass workflow. Choose
the path that matches your goal. All paths use local processes and local state;
Glass does not provide a hosted browser, code service, or autonomous planner.

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

`doctor` does not start Chrome. It reports executable and browser discovery,
platform status, policy, profiles, daemon health, stores, and remediation. A
`degraded` result identifies each missing dependency instead of partially
starting a session.

The two packages intentionally compete for the `glass-browser` command in one
Cargo root. Use the [package transition and complete uninstall
procedures](installation.md#install-from-source) when switching products.

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

2. Inspect the bounded project tree and diagnostics:

   ```console
   glass project files --root .
   glass project diagnostics src/main.rs --root .
   ```

   The tree reports its entry limit, truncation, ignored generated/vendor
   directories, and skipped symlinks. Diagnostics use a persistent LSP client
   in resident sessions and fail explicitly when the language server is not
   installed.

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

Continue with [Development Runtime](development-runtime.md).

## Path B: use the terminal workspace

Start the resident workspace from a project:

```console
cd /path/to/project
glass
```

Use `1`–`6` or `Tab` to switch Overview, Agent, Browser, Project, Diff, and
Process. Press `?` for context-sensitive help. Enter `:` or `/` to discover
commands. The same printable navigation works on narrow SSH terminals.

Useful first commands in the TUI command bar:

```text
project open README.md
project search TODO
project run dev cargo run
project diagnostics src/main.rs
inbox
verify card
```

The resident process owns the editor buffers, PTYs, language services, browser
controller, agent adapter, and event timeline. Browser recovery replaces only
the browser session; project and process state stays alive.

Continuous browser pixels are off by default. `live on` enables a bounded,
ephemeral terminal renderer. `screenshot evidence.png` is a separate explicit
capture. See [Terminal UI architecture](architecture/tui.md) for state and key
ownership.

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
the PTY across detach. Use `live on` for an ephemeral terminal view. Use
`safari` or `browser remote-view open` for a private loopback service forwarded
by the iOS SSH client. Never expose Chrome CDP publicly.

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
| Port `9222` is occupied | Another process owns the preferred CDP port | In TUI use `browser launch --port auto`, or explicitly attach only after Glass verifies the endpoint. |
| Multiple targets found | More than one page is eligible | Run `targets` or `browser targets PORT`, then select an explicit ID/number. |
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
- Revoke Remote View with `browser remote-view close` when sharing ends.
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
