# Development Graph and observable replay

The resident Development Graph connects evidence produced by Glass services.
It is a bounded, revisioned index for answering operational questions such as
which source change was verified by a failing test or which browser regression
was observed by a workflow. It is not hidden model memory and does not contain
private chain-of-thought.

## Typed resources

The shared router projects validated call arguments and result metadata into
typed nodes for repositories, files, symbols, editor revisions, Git changes and
commits, processes, ports, LSP diagnostics, DAP sessions/threads/frames and
breakpoints, browser targets/revisions, Web IR entities, workflow and test runs,
kernels, agents, tasks, experiments, verification, and tool calls. Each node
retains revision, stale state, a bounded label, and bounded observable evidence.

Known service relationships are typed—for example a tool `referencesFile`, a
file `producedEditorRevision`, a debugger `observedThread`, or a browser target
`observedBrowserRevision`. The projector does not infer that two events are
causal merely because one happened later.

## Record explicit causal evidence

`glass.graph.link` connects two existing node IDs with a bounded relation and
evidence object. It requires mutation authority and confirmation because it
changes the shared explanation graph. Provenance is always the authenticated
actor; a request cannot substitute another actor string.

```json
{
  "from": "file:src/checkout.rs",
  "to": "gitChange:repair-7",
  "relation": "changedBy",
  "evidence": {"source": "resident-git-status", "revision": 18}
}
```

Continue with verified links rather than an inferred timeline:

```text
file:src/checkout.rs
  changedBy -> gitChange:repair-7
  verifiedBy -> test:integration-checkout
  observedRegression -> webIr:checkout-submit
```

`glass.graph.path` and `glass.graph.explain` return the nodes and edges. Missing
endpoints, absent paths, stale nodes, oversized evidence, invalid identifiers,
and exhausted bounds are explicit errors.

## Replay contract

Replay records observable events only: authenticated human/agent/tool actions,
file/editor changes, Git observations, process/port state, LSP diagnostics,
debugger state, browser revisions, Web IR/workflow/test results, kernel calls,
task state, and experiment evidence. Tool payloads and results are represented
by bounded metadata such as IDs, revisions, operation names, and byte counts;
prompts, evaluated source, secrets, and private model reasoning are not copied
into replay.

`glass.replay.list` and `glass.replay.inspect` page by sequence. `glass.replay.diff`
returns only the requested increasing range with actor, subsystem, and resource
sets. The in-memory graph and replay use fixed node/edge/event/evidence limits;
older bounded edge/event entries may age out, and revision invalidation marks
older evidence stale rather than silently presenting it as current.

## Interfaces and verification

Pi, MCP, daemon clients, kernels, and the TUI all use the same graph tools. The
Graph and Replay TUI surfaces show the same node/edge/event JSON, including on
compact and phone layouts. The deterministic projection test covers every
issue-35 resource kind and proves a typed source-to-regression path whose events
contain no rationale field populated by model reasoning.
