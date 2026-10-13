# Extensions, mods and skills

[Documentation index](INDEX.md) · [Project overview](../README.md)

Choose the mechanism that matches the work:

| Mechanism | Use |
| --- | --- |
| Project instructions | Repository guidance loaded with consent |
| Skills | Named instructions whose body is read when needed |
| Subprocess extensions | Additional tools, commands, and hooks with their own runtime |
| Compiled mods | In-process hooks registered in the Rust application |
| Agent Plugins | Managed bundles containing skills, hooks, or MCP configuration |

## Project instructions

The host recognizes `AGENTS.md`, `AGENTS.override.md`, and `CLAUDE.md` in the
workspace. The trust decision is associated with the instruction content hash;
changed content requires a new decision. `trust = "always"` bypasses the
interactive gate. Headless runs skip untrusted instructions with a diagnostic
unless explicitly trusted through the supported CLI option.

Instruction selection uses one recognized file per directory. Content is bounded
to 8 KiB per file and 12 KiB in total, with visible truncation. The loader does not
walk arbitrary ancestors. `/context` reports which instructions were supplied.
Instructions describe desired behavior; execution permissions remain separate.

## Skills

Skills are Markdown files under `~/.config/glass-core/skills/`, or `skills_dir`:

```markdown
---
name: release-check
description: Review a release candidate and collect verification evidence.
---

Inspect the release diff. Run the project's release checks and report failures.
Ask for explicit approval before publishing.
```

The catalogue supplies names and descriptions. The model reads a skill body
through tools when needed. `/skills` lists entries; `/skill-doctor` reports
sizes and modification dates. It does not measure model understanding or complete
skill usage. Capturing a skill drafts instructions for review; it does not execute them.

## Subprocess extensions

Extensions communicate through newline-delimited JSON on stdin and stdout.
Dependencies such as Node, Python, Playwright, or an MCP server remain in the
extension process. Enable bundled extensions through `/extensions`, or configure
commands directly:

```toml
extensions = [
  "node /path/to/glass-core/extensions/codemode/codemode.js",
  "python3 /path/to/my-extension.py",
]
```

Custom commands can use a JSON argument vector when quoting would be ambiguous.
Inspect source and access requirements before enabling executable code. See
[Permissions](security.md) for isolation and capability declarations.

Hooks include prompt contribution, authorization, tool execution, result rewriting,
turn completion, and slash commands. Nested `call_tool` requests invoke host tools
under their applicable policy. `extensions/hello.py` is the protocol example;
`crates/glass/src/ext/protocol.rs` defines the host messages.

Each reply must echo the request’s unsigned integer `id`. Missing or malformed
IDs fail the exchange; late replies cannot stand in for a current request.

Every exchange has a timeout and byte limit. Startup failures remain visible as
warnings. If an extension must authorize calls, declare `authorization_required:
true`; missing or invalid replies then block the call. An optional legacy hook
does not provide that guarantee.

Tool declarations are frozen after the first turn to keep the request prefix
stable. New declarations apply in a new session. Disabling an extension stops its
process; calls to its frozen declarations then report unavailability. Duplicate
extension or tool names, including native tool replacements, are rejected.

## TypeScript extensions

The [TypeScript SDK](../packages/extension-sdk/README.md) provides typed tools,
commands, hooks, host calls, session state, previews, and diagnostics. Build the
[starter](../extensions/typescript/README.md) with `npm ci --ignore-scripts` and
`npm run build`, then configure its compiled entry point with Node. The SDK is
available in this source checkout and has not been published to npm.

GC starts the extension's small handshake process once. Tool implementation
modules can load on the first call and remain cached. Node and extension
dependencies stay outside GC's Rust process; disabled extensions add no process.
Previews and diagnostic tables use native Markdown inspection. See the SDK guide
for validation, cancellation, transport bounds, and permission requirements.

## Extension state

`append_custom`, `read_custom`, and `seed_custom` manage namespaced records in the
session log. Startup seeds an extension with its own raw historical entries,
including entries outside the compacted model projection.

An append response acknowledges a queued record. The host persists it when the
current callback completes; a crash before that drain can lose the queued entry.
Use these records for extension state outside model context. Exports identify
their kind, and session switching rebuilds the selected state.

## Compiled mods

Mods implement `glass_mod_api::Mod_`. The dependency-free API defines prompt,
authorization, result, tool, rendering, and input hooks. Mods are registered by
name and compiled into the application; arbitrary dynamic libraries are not loaded.
Native tools remain available when a mod is enabled.

`/mods` manages registered mods. The `redact` mod filters output lines containing
common credential markers. It is best-effort: original output may remain in logs
or spill files, and secrets without recognized markers may pass through.

## Agent Plugins

Plugins package optional components and preserve separate persistent data.
Installation and update actions inspect integrity and capability changes. See
[Integrations](integrations.md#agent-plugins) for the plugin interface and MCP
bridge configuration.

The MCP bridge limits each stdio reply to 8 MiB, matching its HTTP response limit. Oversized replies fail with a transport diagnostic. Plugin integrity checks stream file contents in bounded chunks; existing content hashes and installation receipts remain compatible.

Skill discovery reads the first 64 KiB for front matter and leaves the body on disk until requested. Front matter must close within that prefix. MCP configuration files are limited to 1 MiB; an oversized plugin configuration disables that plugin's MCP entries without hiding its skills or other servers.
