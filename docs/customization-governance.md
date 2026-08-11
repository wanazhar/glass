# Customization governance

Glass loads optional user and trusted-project customization while preserving
where every instruction or executable came from. Workspace trust decides when
project-controlled content may become active; a project file cannot mark itself
trusted.

## Authority classes

| Class | Examples | Effective boundary |
|---|---|---|
| `glassBuiltIn` | runtime rules and built-in tools | shipped Glass code and policy |
| `userGlobal` | files under the user Glass skill directory | local user configuration |
| `trustedProject` | trusted `glass.toml` and `.glass/skills` | repository content explicitly trusted by a local human |
| `untrustedProject` | the same project content before trust | visible for inspection; agent context and execution blocked |
| `externalClient` | CLI, MCP, daemon, or other connected actor | caller identity and negotiated authority; never a customization source |

The Trust TUI serializes inspection items with source, authority, risk, command,
declared mutation behavior, effective policy, and latest evidence. External
clients remain a visible authority boundary even when no client is connected.

## Skill provenance

Agent context labels each active skill by authority and exact source. The TUI
also shows the built-in Glass runtime rules:

```text
SYSTEM   glass-runtime-rules        <glass-built-in>
USER     reviewer                   ~/.config/glass/skills/reviewer.md
PROJECT  project-style              .glass/skills/project-style.md
```

Project skills are visible as `untrustedProject` before trust but are omitted
from the privileged agent instructions. User-global skills remain independent
of project trust.

## Hook evidence

Every configured hook inspection record includes its event, exact command,
source config, timeout, authority, trust requirement, failure policy (`fail` or
`continue`), and latest execution evidence. Evidence contains the actor,
authority, start time, duration, success, ignored-failure status, bounded result
size, and error when present. Hooks run only after trust and are never hidden;
`tool.before`, `tool.after`, and `workspace.opened` results update the same
inspection surface.

## Custom tools and project commands

A custom tool displays its source, executable command, declared `mutating`
value, effective mutation policy, input schema, timeout, trust requirement, and
latest execution evidence. All shell-backed custom tools are effectively
mutating because a command declared read-only can still modify the filesystem
or external state. Consequently every custom tool requires mutation authority
and confirmation; `mutating = false` is retained only as an auditable project
declaration, never as a policy bypass.

Project commands use the same rule and a fixed 15-minute upper bound. The TUI
help/palette labels them `PROJECT:<name>`, and execution results begin with
`PROJECT command <name>` so repository-provided actions are not confused with
Glass built-ins.

## Failure and security

Configuration size, names, commands, schemas, hook counts, timeouts, output,
and execution duration are bounded. Project execution is denied while
untrusted. Commands run as the current OS user and are not sandboxed; use a VM,
container, or restricted account for hostile repositories. Inspection records
never imply that a successful hook or command is safe—only what ran, under
which authority, and what observable result it returned.
