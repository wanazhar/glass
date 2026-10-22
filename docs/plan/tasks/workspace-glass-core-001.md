---
id: workspace-glass-core-001
scope: shared Cargo workspace and synchronized product version
status: done
depends-on: []
---

# Objective

Integrate the imported Glass Core Rust packages into the Glass root Cargo
workspace, make Glass's current version (`0.3.14`) the shared version source,
retain package-specific license metadata, and preserve standalone Core build,
install, binary, alias, web, and extension behavior. Preserve the imported
history; all changes belong in new commits.

# Context

- [Shared workspace design](../analysis/shared-workspace.md)
- [Glass documentation index](../../INDEX.md)
- [Maintainer handbook](../../maintainers/README.md)
- [Imported product guide](../../../products/glass-core/README.md)
- [Migration evidence](../../migration/glass-core-monorepo.md)

# Paths

- `Cargo.toml`, `Cargo.lock`
- `crates/glass-core/`, `crates/glass-mod-api/`,
  `crates/glass-runtime-client/`, `crates/glass-tui/`
- `products/glass-core/` product assets, scripts, installer, package files,
  docs, and CI workflows
- workspace/release/version validators and current product documentation
- documentation inventory and navigation files affected by moved paths

# Verification

- `cargo metadata --locked --format-version 1` lists all expected packages and
  resolves a single root lockfile.
- `cargo tree -p glass-core --locked` proves no dependency on
  `glass-dev`, `glass-browser`, or `glass-browser-core`.
- Build and focused-test the Core package without building the Glass product;
  include regressions for the five imported baseline failures.
- Run installer and alias checks under an isolated install root; verify
  embedded/runtime assets and extension/web paths.
- Run relevant Glass package checks, formatting for touched Rust files,
  strict Clippy on touched packages, docs inventory/depth/coverage, and the
  shared version synchronization validator.
- Verify all preserved imported source commits remain ancestors of `main` and
  retain their original object IDs.
