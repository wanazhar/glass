# Public documentation and docs.rs audit

Status: Complete

## Scope

Audit every shipped documentation surface against the `0.3.2` implementation:

- repository and package READMEs;
- user, operator, SDK, MCP, TUI, security, and maintainer guides;
- crate-level and module-level rustdoc;
- CLI command and MCP tool inventories;
- examples, Cargo features, schemas, support status, and release evidence;
- local Markdown links and packaged documentation.

Historical delivery records under `docs/plan/tasks`, `docs/plan/reviews`, and
past release analyses are evidence, not current user reference material. They
are checked for valid links but are not rewritten to describe later behavior.

## Confirmed gaps

| Surface | Finding | Required amendment |
|---|---|---|
| `glass-browser` package README | Twelve lines and no SDK workflow | Complete crates.io landing page with safety, features, examples, and links |
| `glass-dev` package README | Minimal product explanation and repository-relative link | Complete executable/TUI/MCP entry page with permanent links |
| Crate root rustdoc | Browser-session quick start only and incomplete module map | Audience routing, ownership, semantic pipeline, compiled examples, and all modules |
| Rust SDK guide | Missing | One technical guide for browser, semantic, task, knowledge, backend, and development APIs |
| MCP reference | Main table does not enumerate the complete negotiated inventory | Dedicated catalog covering every checked-in conformance tool |
| CLI reference | Several top-level and nested command families absent | Complete command-family catalog tied to generated `--help` |
| Examples | Seventeen examples have no discoverable catalog | Requirements, invocation, outputs, and claim boundaries for each example |
| Documentation index | Flat list optimized for maintainers | Role-based starts plus exhaustive reference index |
| Drift prevention | Release validator checks markers but not complete inventories or local links | Automated CLI, MCP, example, module, and link coverage gate |

## Acceptance

1. Every top-level CLI command printed by both binaries is named in the CLI
   reference.
2. Every MCP tool in the checked-in client conformance fixture is named in the
   MCP tool catalog.
3. Every Cargo example is named with a runnable command and environment class.
4. Every public crate module is routed from crate-level rustdoc.
5. Every public Markdown file has valid local links; historical plan files may
   link only to existing paths or external URLs.
6. Rust examples compile as doctests and standard all-feature rustdoc is
   warning-free.
7. Both Cargo packages contain self-sufficient README content with permanent
   repository documentation links.
