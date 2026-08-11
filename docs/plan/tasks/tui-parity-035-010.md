# TUI parity and recovery surfaces

Status: Complete locally

## Contract

Expose every issue-35 resident development surface directly in the Ratatui
workspace, retain governed router authority for actions, and keep trust and
recovery decisions available without requiring a CLI exit. Phone and compact
layouts must support drill-down rather than squeezing desktop panels into a
narrow pane.

## Implementation

- Added first-class LSP, Workflow, Kernels, and Daemon/Workspace surfaces to
  the existing Trust, Agent, Tasks, Editor, Processes, Browser, Debugger, Git,
  Tests, Experiments, Graph, and Replay navigation.
- Projected bounded resident LSP, workflow, kernel, workspace identity, trust,
  generation, task, and agent state into those surfaces.
- Kept action commands on `DevelopmentToolRouter`; the Daemon/Workspace route
  is inspection-only and does not invent separate daemon semantics.
- Ordered the narrow overview around workspace, tasks, agents,
  semantic/browser evidence, runtime health, and recovery/trust decisions.

## Evidence

`cargo test -p glass-dev --lib tui:: --all-features` renders every surface at
desktop, compact, and phone geometries and separately verifies the phone trust
gate. Strict all-target/all-feature Clippy passes for the crate.
