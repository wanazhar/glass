# Measured experiments

Glass experiments compare isolated Git worktrees using recorded results.
Experiment creation requires a trusted workspace. A child worktree receives
trust only for the lifetime of its experiment; Glass does not save a new trust
record for it.

Use the TUI command palette:

```text
:experiment list
:experiment create approach-a experiment-a 3101
:experiment collect approach-a
:experiment compare
:experiment select approach-a
```

The shared router exposes `glass.experiment.create`, `collect`, `list`,
`compare`, `select`, and `remove` to MCP, daemon, and resident-agent clients.
Resident agents use Glass Core and the same router.

## Automatic providers

`collect` runs configured build and test commands with time and output limits.
It records current Git changes, process startup and crash state, LSP
diagnostics, and debugger stops. If a browser or workflow is attached, it also
records workflow verification and semantic differences. Browser visual and
performance metrics stay unavailable until their provider returns current
evidence. Glass does not treat missing measurements as zero.

Every evidence family records:

- producer and observation timestamp;
- workspace and optional browser revision;
- run ID where available;
- measured versus manual origin;
- availability; and
- bounded provider details.

Manual evidence remains accepted for compatibility, but Glass overwrites its
provenance to `manual-external` and `measured: false`; a caller cannot relabel
manual numbers as resident measurements.

## Ranking

Ranking is deterministic. Tests, workflow verification, semantic regressions,
diagnostics, change size, process crashes, build/startup health, visual
difference, and LCP contribute fixed default weights. Custom weights have a
bounded magnitude and can be applied only after an explicit trust decision.
Agents cannot change weights through the experiment evidence payload.

The More TUI surface shows the current snapshot summary and evidence-derived
recommendation; use `:experiment list` for an explicit refresh. Selection
rejects any candidate that is not the current evidence-derived winner.
