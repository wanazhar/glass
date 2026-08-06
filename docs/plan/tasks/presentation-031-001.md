---
id: presentation-031-001
scope: Browser presentation scheduling foundation
status: ready
depends-on: []
---

# Objective

Define browser-neutral presentation contracts for bounded frame delivery, viewport geometry, ownership, and measurable latency. The foundation must make latest-frame replacement and independent capture-scale/frame-rate control possible without coupling the TUI to CDP or a terminal image protocol.

# Context

- `docs/plan/analysis/release-031.md`
- `docs/architecture/tui.md`
- Issue #31 Pillar IV amendments, sections IV.6–IV.15, and Gate 4
- `src/tui/app.rs`
- `src/browser/session/screenshot.rs` or existing capture/session modules

# Path

- new `src/presentation.rs` or `src/presentation/` module
- focused tests beside the module
- module-local contract doc if needed

Do not edit `src/lib.rs`, `src/tui/app.rs`, CDP code, or terminal backends in this foundation task.

# Contract

Provide bounded types for:

- `BrowserFrame` metadata: generation, target/resource identity, acquisition timestamp, viewport/content dimensions, scale, encoding, keyframe, damage, browser revision, and dropped counts;
- viewport/pane pixel geometry and stale-snapshot detection;
- latest-frame mailbox retaining the current frame and at most one newest pending frame, with replacement/drop counters;
- presentation mode and degradation reasons;
- independent target frame rate and capture scale;
- presentation metrics including acquire/present/input latency, ACK delay, dropped/stale frames, bytes, and pending count;
- explicit cleanup/ownership events that do not persist frame bytes by default.

The mailbox must never expose an unbounded queue. Frame/revision synchronization must be explicit; stale semantic overlays cannot be treated as current by this contract.

# Verification

Run focused tests for mailbox bounds/replacement, geometry conversion and stale rejection, metric updates, scale/rate validation, and serde limits. Do not run formatters, linters, or project-wide test suites. Commit with `feat(presentation): ...` before handoff.
