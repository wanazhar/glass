# Documentation

Start with the [product overview](../README.md) or [Getting started](getting-started.md).
These guides describe the current interface. Commands also provide focused help
through `gc help` and `gc COMMAND --help`.

## User guides

| Guide | Contents |
| --- | --- |
| [Getting started](getting-started.md) | Install, connect a provider, resume work |
| [Terminal interface](terminal.md) | Keys, palette, scrolling, output, appearance |
| [Conversations and context](sessions.md) | Sessions, attachments, recovery, compaction |
| [Native tools and jobs](tools.md) | File revisions, previews, validation, repair, processes |
| [Coding workflows](workflows.md) | Plans, changes, checkpoints, tasks, artifacts |
| [Agents](agents.md) | Delegation, persistent teammates, model-assisted review |
| [Project knowledge](knowledge.md) | Neuralnets, reviewed facts, migration, guidance |
| [Configuration](configuration.md) | Providers, profiles, fallback, cache reports, budgets |
| [Permissions](security.md) | Policy layers, confinement, approvals, limits |
| [Optional integrations](integrations.md) | Web, MCP, plugins, verification, language services |
| [Extending](extending.md) | Instructions, skills, subprocesses, compiled mods |
| [Automation](automation.md) | Headless results, SSH, ACP, served sessions, schedules |

## Detailed references

- [Host runtime protocol](reference/runtime.md): drive GC from an editor or agent
  environment with host-owned tools, controls, structured events, typed clients,
  incremental sessions, and isolated completion.

- [Session checklist](reference/checklist.md): phased tasks, state operations,
  persistence, and continuation.
- [Model guidance](reference/model-guidance.md): workflow selection, checklist
  instructions, and discovery.
- [Approvals and user questions](reference/interaction.md): paused decisions,
  execution modes, and native questions.
- [Workspace](reference/workspace.md): tasks, roles, worker limits, request metadata,
  findings, policy explanations, and artifacts.
- [Workbench](reference/workbench.md): quoted context, portable handoffs, job input,
  PR publication, advisor cadence, schedules, and optional clients.
- [Storage and request overhead](reference/storage.md): automatic writes, capture
  limits, retention, and footprint measurements.
- [Confinement](security-confinement.md): enforced filesystem/network boundaries
  and platform requirements.
- [Extension directory](../extensions/README.md): component-specific configuration
  and access.

## Development and records

- [Development](development.md): architecture, measurements, builds, and verification.
- [Writing guide](writing.md): language, structure, examples, and evidence rules.
- [Implementation records](plan/README.md): design documents, tasks, and dated reviews.
- [Research snapshots](../research/README.md): external-source investigations and roadmaps.

Historical records retain their original scope, test counts, and evidence. They
are not the current user reference. See a topic guide first when behavior differs.
