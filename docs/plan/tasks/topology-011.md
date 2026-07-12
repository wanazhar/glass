---
id: topology-011
scope: browser targets and frames
status: done
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

Implemented evidence:

- Real Chrome: user-gesture popup discovery, explicit tab switching, active
  close, target crash, nested offset-frame click, forced cross-site OOPIF
  evaluation/click, and CLI/MCP frame-routed evaluation.
- Route race: a unit contract changes global selection between two CDP calls
  and proves the in-flight operation retains its original session.
- Resource sanity ([retained JSON](../../../benchmarks/topology-report.json), five
  local release iterations): Glass RSS 3.2 MiB before start, 4.8 MiB after one
  session, and 7.1 MiB after the two-session workload; compact cached
  observation p50/p95 0.015/0.021 ms, fresh p50/p95 4.83/5.00 ms, compact
  payload 16,021 bytes, 133 CDP requests after the workload, and a 4,726,808
  byte release binary. These are local regression evidence, not cross-machine
  claims.
