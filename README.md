# glass-core

**A coding agent for your terminal, built in Rust.**

glass-core reads your project, edits files, runs commands, and keeps the work in a
resumable conversation. Use it on your desktop, over SSH, or from a script. The
short command is `gc`.

The terminal interface keeps code, tool output, reasoning summaries, and your draft
in separate views. You can inspect a running command, steer the task, or open a
teammate's transcript while work continues.

## Why glass-core

- **Fast local interaction.** Bounded output, cached transcript layout, and lazy
  optional integrations keep typing and navigation responsive.
- **Edits you can inspect.** Native read, edit, and write tools provide revisions,
  previews, and supported syntax checks. Stale revisions are rejected.
- **A readable workspace.** Searchable conversations, formatted output, change
  review, and contextual actions share one terminal interface, including small
  SSH terminals.
- **Agents you can follow.** Persistent teammates retain their context. Review
  their messages and transcripts, send a follow-up, or interrupt their work.
- **Context you can account for.** Inspect supplied instructions, attachments,
  tool schemas, request usage, cache reports, and saved project knowledge.
- **Extensions in TypeScript.** Add typed tools and commands with native previews
  and diagnostics. Implementations can load on first use in a persistent process.
  See the [TypeScript SDK](packages/extension-sdk/README.md).
- **Your choice of connection.** Use Anthropic Messages, OpenAI Chat Completions,
  or Responses endpoints, including compatible local runtimes. Authentication
  supports API keys and ChatGPT sign-in.

## Get started

From a source checkout, with Rust 1.88 or newer:

```sh
./scripts/install.sh
gc
```

Open `/login` to connect an account, then `/model` to select a model. Start with a
specific task:

```text
Find the cause of the failing parser test. Explain the bug, then fix it.
```

The installer creates `glass-core` and a relative `gc` symlink. It preserves
unrelated commands, including the separate `glass` project. For a build without
installation, see [Getting started](docs/getting-started.md).

## Stay in control of the work

Type `/` or press **Ctrl+P** to search actions. The three main destinations are
**Conversations**, **Work**, and **Settings**.

| Task | Open |
| --- | --- |
| Resume or branch a conversation | `/sessions`, `/branches`, or `gc resume` |
| Read tool output or copy code | `/outputs`, `/responses`, or `/details` |
| Inspect changes before a commit | `/diff`, `/review`, or `/git` |
| Follow a teammate's work | `/agents` or **Alt+A** |
| Inspect supplied context and usage | `/context` or `/usage` |
| Review permissions and integrations | `/security` or `/extensions` |

**Ctrl+G** opens the transcript. **Ctrl+T** opens reasoning or summaries exposed by
the provider. **Esc** returns to the previous menu or closes the top-level view and restores your
draft. During a turn,
**Enter** submits steering and **Alt+Enter** queues a follow-up.

## Use it beyond the terminal

Run a task without a terminal, resume saved work, or supply a prompt through stdin:

```sh
gc run --json "Explain the repository's test setup"
gc run --json --resume SESSION_ID "Continue the task"
printf '%s' 'Review src/parser.rs for EOF handling' | gc run --json
```

Headless runs save resumable sessions. JSON results go to stdout; diagnostics go
to stderr. Optional ACP, served sessions, and companion integrations connect the
same runtime to other interfaces. With `gc runtime`, your editor or agent
environment can supply its own tools and UI while GC runs the model loop.
Use the [Rust client](crates/glass-runtime-client/README.md),
[TypeScript client](packages/runtime-client/README.md), or any JSON-RPC host.
The [host protocol](docs/reference/runtime.md) covers structured sessions, images,
progress, cancellation, and isolated completion. See [Automation](docs/automation.md)
for other headless interfaces.

### Continue from your phone

With the optional web assets installed, `gc connect` opens a private Tailscale
HTTPS workspace. Install it on your iPhone's Home Screen and continue the same
threads, inspect rich previews, answer questions, or approve an operation while
work runs on your host. The web interface loads heavy preview renderers only when
needed. See [Remote workspace](docs/connect.md) for setup and access controls.

## Extend the workflow

The core exposes `read`, `edit`, `write`, `view_image`, `bash`, and `state`.
Interactive models also receive `ask_user_question`, with choices, multiple
selections, and a custom answer field. Text-only models omit `view_image`.
Agents can inspect screenshots and project images without reading binary files as text.
Native checklists keep phased progress and blockers visible, survive resume and
compaction, and open in `/todo` without an extension.
Additional workflows are discovered on demand. Optional subprocess extensions add
web search, MCP tools, browser checks, language services, and other integrations.
Skills and compiled mods provide instructions and hooks.

Neuralnets lets agents save and recall attributed project knowledge directly.
You can inspect, edit or forget memories; local recaps make no provider requests.
See [Project knowledge](docs/knowledge.md) and [Extending glass-core](docs/extending.md).

## Permissions

The default profile is unrestricted, with sandboxing off. Start with
`gc --restricted` for workspace confinement and explicit approval requests, or
`gc --yolo` for unrestricted execution. Questions and approval dialogs pause the
turn until you respond. Review extensions before enabling them. Global policy,
permission profiles, command approvals, and extension isolation have distinct
roles. [Permissions and security](docs/security.md) explains their boundaries.

## Documentation

| Start here | Reference |
| --- | --- |
| [Getting started](docs/getting-started.md) | Installation, authentication, first task |
| [Terminal interface](docs/terminal.md) | Keys, scrolling, output, motion |
| [Coding workflows](docs/workflows.md) | Plans, review, checkpoints, tasks, artifacts |
| [Configuration](docs/configuration.md) | Providers, caching, pricing, budgets |
| [Native tools](docs/tools.md) | File operations, repair, background jobs |
| [Agents](docs/agents.md) | Delegation, persistent teammates, review |
| [Development](docs/development.md) | Architecture, measurements, verification |

The [documentation index](docs/INDEX.md) covers all guides and implementation
records. Run `gc help` or `gc COMMAND --help` for the installed command interface.
