---
id: development-032-003
scope: native TUI and harness
status: pending
depends-on: [development-032-002]
---

# Objective

Add a first-class Development workspace mode to the existing Ratatui TUI and
connect a deterministic Glass-owned harness adapter. The layout must expose
files/editor, live browser status, processes, diagnostics/activity, actors,
and the command surface while preserving the existing browser workspace.

# Context

- `docs/plan/analysis/release-032.md`
- Issue #32 TUI mock designs A–E
- `docs/architecture/tui.md`
- `/home/ubuntu/.agents/skills/docs-sprint/references/ui-layout.md`

# Path

- `src/tui/`
- `src/development/agent.rs`
- `docs/architecture/development-tui.md`

# Verification

- parser and state transition tests for the development command palette;
- harness tests for prompt/stream/tool/steer/error events;
- layout documentation with ASCII diagrams;
- terminal smoke checks where available;
- `cargo test --locked` and Clippy.
