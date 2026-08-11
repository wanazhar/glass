# Measured experiments

Glass experiments compare isolated Git worktrees using observable evidence,
not an agent's preferred narrative. Experiment creation requires workspace
trust and child worktrees inherit only process-lifetime `TrustedOnce` authority.

Use the TUI command palette:

```text
:experiment create approach-a experiment-a 3101
:experiment collect approach-a
:experiment compare
:experiment select approach-a
```

The shared router exposes matching `glass.experiment.create`, `collect`,
`list`, `compare`, `select`, and `remove` operations to MCP, daemon, and Pi
clients.

## Automatic providers

`collect` runs configured build and test commands with bounded execution and
collects the current Git change surface, resident process startup/crash health,
LSP diagnostic observations, debugger stops, and—when a browser/workflow is
attached—workflow verification and semantic diff evidence. Browser visual and
performance metrics are explicitly marked unavailable until their provider has
current evidence; they are never converted into favorable zeroes.

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

The Experiments TUI shows the complete snapshots and provenance alongside the
recommendation. Selection rejects any candidate that is not the current
evidence-derived winner.
