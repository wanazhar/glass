---
id: development-032-001
scope: project runtime core
status: completed
depends-on: []
---

# Objective

Implement the bounded project runtime core described in
`docs/plan/analysis/release-032.md`: project detection/configuration, safe file
and editor operations, PTY process lifecycle, development events/timeline,
source/runtime graph links, and deterministic impact projections.

# Context

- `docs/INDEX.md`
- `docs/plan/analysis/release-032.md`
- Issue #32, stages A–E and the explicit non-goals
- `SECURITY.md`

# Path

- `crates/glass-browser/src/development/`
- `crates/glass-browser/src/lib.rs`
- `Cargo.toml`
- focused unit tests beside the new modules

# Verification

- `cargo fmt --all -- --check`
- focused `cargo test development`
- `cargo test --locked`
- tests cover root confinement, bounded reads, config overrides, PTY
  lifecycle/output, event persistence, provenance/confidence, and stable diff
  serialization.
