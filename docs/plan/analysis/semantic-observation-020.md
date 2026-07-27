# Semantic observation engine (`#22`)

## Scope

Issue #22 evolves the existing accessibility-first observation path into a
bounded semantic model. Work is serial and local; no public `0.1.20` release,
push, or registry publication is part of this implementation.

## Phase decomposition

1. **Contract** — versioned page, region, route, confidence, evidence, limits,
   and expansion-handle types with canonical JSON and a published schema.
2. **Classification** — derive deterministic regions and page kinds from
   existing AX/DOM/topology evidence without hiding actionable targets.
3. **Levels and expansion** — add summary/interactive/structured levels and
   bounded region, collection, table, frame, and neighborhood expansion.
4. **Diffs and continuity** — add compatible-revision semantic changes and
   conservative identity continuity without bypassing guarded references.
5. **Interfaces** — expose the same contract through Rust, CLI, MCP,
   TypeScript, Python, and the TUI.
6. **Validation and preparation** — add fixture matrices, payload metrics,
   privacy coverage, migration docs, and a local-only milestone tag.

## Contract boundaries

- Semantic labels are advisory and evidence-backed; they never authorize an
  action by themselves.
- `unknown` and `generic` are valid classifications.
- Existing detailed/raw observations remain available.
- Every semantic handle is scoped to a route and observation revision.
- Bounds, omissions, and redaction are explicit in the returned contract.
