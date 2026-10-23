# Customization governance

Glass records the source of user and project customization, tool declarations,
and execution evidence. Glass Core owns native coding tools and their
permissions. Glass does not mirror Core's coding permission policy in its
workspace UI. Glass-owned integrations retain actor attribution, revision and
path checks, and per-call confirmation for operations that require it.

## Authority classes

| Class | Examples | Effective boundary |
|---|---|---|
| `glassBuiltIn` | runtime rules and built-in tools | shipped Glass behavior |
| `userGlobal` | files under the user Glass skill directory | local user configuration |
| `project` | `glass.toml` and `.glass/skills` | project configuration, labeled with its exact source |
| `externalClient` | CLI, MCP, daemon, or other connected actor | caller identity and actor attribution |

Workspace inspection reports retain source, declared behavior, and latest
evidence. These records explain what configuration was loaded and what ran;
they do not control whether the workspace opens or duplicate Core's native
coding permissions.

## Skill provenance

Agent context labels each active skill by its exact source. The TUI also shows
the built-in Glass runtime rules:

```text
SYSTEM   glass-runtime-rules        <glass-built-in>
USER     reviewer                   ~/.config/glass/skills/reviewer.md
PROJECT  project-style              .glass/skills/project-style.md
```

Project and user skills remain distinct in the context record. Glass Core
applies its own permissions to its native coding tools; Glass passes workspace
context and does not add an Ask/Plan/Agent mode or a second coding permission
layer.

## Hook evidence

Every configured hook inspection record includes its event, exact command,
source config, timeout, failure policy (`fail` or `continue`), and latest
execution evidence. Evidence contains the actor, start time, duration, success,
ignored-failure status, bounded result size, and error when present. The
inspection surface keeps these records available for review.

## Custom tools and project commands

A custom tool displays its source, executable command, declared `mutating`
value, input schema, timeout, and latest execution evidence. Shell-backed
commands run with the launching user's OS permissions, so the declaration is
descriptive and does not sandbox the command. Keep source, actor, and result
visible for review.

Workspace tools use the shared router so Glass can retain actor attribution,
path confinement, revision checks, and integration confirmation. Core's native
coding tool authorization remains Core-owned.

Project commands have a fixed 15-minute upper bound. The TUI help/palette labels
them `PROJECT:<name>`, and execution results begin with
`PROJECT command <name>` so repository-provided actions are not confused with
Glass built-ins.

## Failure and security

Configuration size, names, commands, schemas, hook counts, timeouts, output,
and execution duration are bounded. Commands run as the current OS user and are
not sandboxed; use a VM, container, or restricted account for hostile
repositories. Inspection records never imply that a successful hook or command
is safe—only what ran, under which actor, and what observable result it
returned.
