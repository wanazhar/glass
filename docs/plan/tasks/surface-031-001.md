---
id: surface-031-001
scope: Multi-surface Web IR contract foundation
status: ready
depends-on: []
---

# Objective

Define a bounded, transport-neutral surface contract that lets Glass represent browser-hosted surfaces beyond one HTML document without claiming unsupported semantics. The contract is a foundation for later Web IR extraction integration; do not fake extraction in this task.

# Context

- `docs/plan/analysis/release-031.md`
- `docs/semantic-execution.md`
- `docs/semantic-observation.md`
- Issue #31 sections II, cross-pillar integration, and Gate 2
- `src/web_ir.rs`
- `src/extraction.rs`

# Path

- new `src/surfaces.rs` (or the smallest existing module location justified by repository conventions)
- focused tests beside the module
- a module-local design contract doc if needed

Do not edit `src/lib.rs`, `src/web_ir.rs`, extraction dispatch, CLI/MCP, or TUI in this foundation task.

# Contract

Provide bounded serde types for:

- stable surface identity and parent/nesting relationship;
- initial surface kinds: document, accessibility, shadow/frame document, SVG, Canvas 2D, WebGL, WebGPU, embedded document/PDF, media, terminal, remote application, browser-native, extension-defined, unknown, opaque;
- capability classes (read structure/text/relations/state, semantic or coordinate action, input, capture, extraction, bridge, revision observation, verification);
- explicit understanding levels 0 opaque through 4 task-compilable;
- coverage, evidence sources, revision, diagnostics, and namespaced extension identifiers;
- bounded validation errors for unknown kinds, duplicate capabilities, invalid IDs, excessive nesting/payloads, and incompatible levels.

The contract must not import CDP-specific IDs or imply that detection grants semantic action. Unknown extensions remain serializable and inspectable only after schema validation.

# Verification

Run focused surface contract unit tests covering serde round trips, invalid input, bounds, capability/understanding invariants, and extension namespaces. Do not run formatters, linters, or project-wide test suites. Commit with `feat(surface): ...` before handoff.
