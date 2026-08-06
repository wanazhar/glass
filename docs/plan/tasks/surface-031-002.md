---
id: surface-031-002
scope: Web IR multi-surface extraction integration
status: pending
depends-on: [surface-031-001]
---

# Objective

Integrate the surface contract into bounded extraction and Web IR serialization so one page can expose document, accessibility, shadow/frame, SVG/graphics, embedded, media, native, bridge, and opaque boundaries without regressing current HTML/AX behavior.

# Context

- `docs/plan/analysis/release-031.md`
- `docs/semantic-execution.md`
- `docs/semantic-observation.md`
- Issue #31 Pillar II and Gate 2
- `src/surfaces.rs` from `surface-031-001`
- `src/web_ir.rs`
- `src/extraction.rs`

# Path

`src/surfaces.rs`, `src/web_ir.rs`, `src/extraction.rs`, relevant browser session extraction modules, `tests/fixtures/web-ir/`, `tests/web_ir_corpus.rs`, and docs/INDEX or design docs as required.

# Verification

Use real fixtures for a multi-surface page, Canvas/WebGL/WebGPU detection, one structured non-DOM adapter, opaque fail-closed compilation, and serde/canonical-diff compatibility. Run focused tests only during development; independent review is required before merge.
