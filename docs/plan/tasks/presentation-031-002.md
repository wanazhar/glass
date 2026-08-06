---
id: presentation-031-002
scope: Terminal graphics and frame presentation integration
status: pending
depends-on: [presentation-031-001]
---

# Objective

Integrate the browser-neutral presentation contract with a bounded terminal graphics abstraction and Ratatui-compatible placement/cleanup. Implement Kitty as the primary path and a usable semantic/ANSI fallback; add another pixel path only where the contract can be honored.

# Context

- `docs/plan/analysis/release-031.md`
- `docs/architecture/tui.md`
- Issue #31 Pillar IV amendments and Gate 4
- `src/presentation.rs` from `presentation-031-001`
- `src/tui/app.rs`

# Path

New `src/terminal_graphics/` and presentation adapters, TUI placement/cleanup integration, capability detection, frame diagnostics, and focused tests. Keep terminal rendering separate from browser control and do not import CDP types.

# Verification

Exercise bounded latest-frame replacement, pane resize/cleanup, capability negotiation, semantic fallback, and no-artifact shutdown. Browser-backed smoke must use the existing Glass-controlled target; pixel performance must be measured rather than claimed.
