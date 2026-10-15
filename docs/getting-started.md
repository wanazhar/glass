# Getting started

[Documentation index](INDEX.md) · [Project overview](../README.md)

## Install

glass-core supports Linux and macOS. Building from source requires Rust 1.88 or
newer and a C toolchain. Optional extensions require their own Python or Node
runtime.

From a source checkout:

```sh
./scripts/install.sh
glass-core --version
gc --version
```

The Cargo package and executable are named `glass-core`. The installer uses
`${CARGO_INSTALL_ROOT:-${CARGO_HOME:-$HOME/.cargo}}/bin` and creates `gc` as a
relative symlink. It refuses to replace an unrelated `gc` command. The separate
`glass` package and executable belong to another project.

To build without installing:

```sh
cargo build --release --package glass-core --bin glass-core
./target/release/glass-core
```

Run the command from the project you want to work on. Git is optional for chat
and tool execution; Git-based checkpoints and change review require a repository.

## Choose an execution mode

Running `gc` uses saved settings. With no configured override, the default is
`unrestricted` with sandboxing off.

```sh
gc --restricted  # Workspace confinement with explicit approval requests
gc --yolo        # No local sandbox or permission prompts
```

Administrator requirements and explicit policy denials apply in both modes.
Approval dialogs pause the turn and show the requested operation before execution.
Models can also ask questions with choices or a written answer. Question answers
do not grant execution rights. See [Approvals and user questions](reference/interaction.md)
for exact scopes, platform limits, and headless behavior.

## Provider onboarding

Open `/login` to connect a provider or account, then `/model` to select a model.
The dialogs support masked secret entry and cancellation. Use `/status` to inspect
the selected connection.

Supported adapters are Anthropic Messages, OpenAI Chat Completions, and OpenAI
Responses. The login picker includes hosted providers and local runtimes. A custom
endpoint can use any supported adapter; individual endpoint features still depend
on the server and model.

API keys can come from an environment variable or private local storage. Local
runtimes can use no authentication. ChatGPT sign-in uses a browser authorization
flow; on a remote terminal, paste the final callback URL when prompted. Tokens
refresh before new turns. Revoked sessions require sign-in again.

Saved connections and credentials use `~/.config/glass-core/auth.json`, or the XDG
config location, with private atomic writes on Unix. Multiple accounts are stored
separately. glass-core does not import credentials from other coding agents.
`/logout` removes the saved connection from this application; it does not revoke
the provider grant or unset an environment variable.

Missing credentials are reported before inference. Provider failures preserve
available partial output and offer recovery through `/retry`. A model without a
rate card shows usage with an unavailable cost, rather than a guessed price.

For direct configuration and local endpoint examples, see
[Configuration](configuration.md).

## CLI help

Run `gc help` for a quick start. Use `gc help commands`, `gc help run`, or
`gc help keys` for focused command, headless, and keyboard guides; `gc help all`
shows the full guide. Help adapts to terminal width and uses plain text when
piped or when `NO_COLOR` is set.

## Consistent command output and resuming

Human-facing commands (`gc status`, `gc doctor`, `gc sessions`, `gc profiles`,
`gc extensions`, file tools, teammates, plugins, worktrees and server commands)
share the help palette, readable labels and terminal-width wrapping. Use
`gc COMMAND --help` for a focused guide. `--json` and protocol streams keep
machine-readable output; exported Markdown remains a plain document.

`gc resume` opens a searchable picker for the current project's conversations,
newest first. Type a title or thread ID, use arrows and Enter to resume, or Esc
or Ctrl+C to cancel. F1 shows the full selected label. `gc resume ID` opens a
specific conversation directly. Over SSH without a terminal, use
`gc sessions --json` and `gc run --resume ID <prompt>`.
