---
id: topology-011
scope: browser targets and frames
status: in-progress
depends-on: [wait-010]
---

# Model tabs, popups, targets, and frames

## Objective

Introduce a bounded single-owner topology registry and explicit handles for
page targets and frames without penalizing the common one-page path.

## Context

- `docs/architecture/automation.md`
- `docs/architecture/browser.md`

## Path

- `src/browser/cdp.rs`
- `src/browser/session.rs`
- `src/browser/chrome.rs`
- `src/cli/`
- `src/mcp/`
- `tests/`
- `docs/architecture/`

## Requirements

- List, create, select, and close page targets explicitly.
- Track popup, close, crash, and frame lifecycle events.
- Route commands through target/frame identity, including cross-origin frames.
- Never change the active target implicitly.
- Bound retained topology and event history.

## Verification

- Real-Chrome popup, multi-tab, nested-frame, cross-origin-frame, close, and
  crash scenarios.
- CLI and MCP integration from target discovery through frame action.
- One-page observation/action resource budget remains within its gate.
