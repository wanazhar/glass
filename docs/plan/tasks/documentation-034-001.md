---
id: documentation-034-001
scope: complete public documentation and docs.rs revamp
status: complete
depends-on: [semantic-resource-033-002]
release: 0.3.2
---

# Revamp all public documentation surfaces

## Context

- `docs/plan/analysis/documentation-revamp-034.md`
- `docs/documentation-style.md`
- the actual CLI help, MCP conformance fixture, Cargo metadata, public modules,
  examples, feature matrix, and release validators

## Path

- root and package READMEs;
- `docs/INDEX.md` and all current user/maintainer references;
- `crates/glass-browser/src/lib.rs` and public module entry documentation;
- new SDK, feature, MCP tool, and example references;
- documentation coverage validator and release checks.

## Verification

- generated command/tool/example/module inventories match documentation;
- all local Markdown links resolve;
- every Rust snippet and doctest compiles;
- strict all-feature rustdoc, release documentation checks, client smokes, and
  both package listings pass;
- conventional local commits only; no push, tag, publication, or release.

## Result

- Public entry points are role-based and package-safe.
- CLI, MCP, example, and public-module inventories are complete and enforced by
  `scripts/check-documentation-coverage.py`.
- All repository-local Markdown links are checked, including historical
  planning evidence without rewriting its original claims.
- Crate and module rustdoc explain ownership, safety, structured observation,
  semantic compilation, optional features, and every public module route.
