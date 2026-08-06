---
id: backend-031-001
scope: Browser Capability Interface foundation
status: ready
depends-on: []
---

# Objective

Define the transport-neutral Browser Capability Interface contracts that Glass core, compiler policy, presentation, and future backends can consume without importing CDP types. This task defines the boundary and capability profiles; it does not refactor the existing CDP session or implement WebDriver BiDi.

# Context

- `docs/plan/analysis/release-031.md`
- `docs/architecture/browser.md`
- `docs/semantic-execution.md`
- Issue #31 sections III, cross-pillar integration, and Gate 3
- `src/browser/cdp.rs`
- `src/browser/session/mod.rs`

# Path

- new `src/browser_backend.rs` or `src/browser_backend/` module
- focused tests beside the module
- module-local design contract doc if needed

Do not edit `src/lib.rs`, `src/browser/cdp.rs`, `src/browser/session`, CLI/MCP dispatch, or TUI in this foundation task.

# Contract

Provide bounded, serde-stable types for:

- semantic backend capabilities and support levels (`available`, `partial`, `unavailable`, `restricted`);
- backend identity/version/browser range and certification profile (`productionCertified`, `experimental`, `partial`, `unsupported`);
- capability dependencies and portable/backend-specific classifications;
- deterministic backend selection inputs/results;
- stable backend errors that distinguish unavailable capability, invalid configuration, connection/lifecycle failure, and unsupported operation;
- an async-friendly `BrowserBackend` boundary expressed in Glass terms (navigation, contexts, evidence, action, effects, script, capture, storage, prompts, downloads), without CDP command/node/target/domain types.

Required fields must fail fast; absent optional capabilities remain explicit. A capability omission must not silently downgrade an operation.

# Verification

Run focused backend contract tests for serialization, selection precedence, capability omission, certification validation, and typed errors. Do not run formatters, linters, or project-wide test suites. Commit with `feat(backend): ...` before handoff.
