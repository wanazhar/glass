---
id: input-012
scope: complete browser input primitives
status: in-progress
depends-on: [topology-011, target-009]
---

# Complete pointer, keyboard, form, and upload input

## Objective

Add the minimum complete primitive set for modern web workflows while sharing
the deterministic locator, topology, policy, and action-outcome contracts.

## Context

- `docs/architecture/automation.md`
- `docs/architecture/browser.md`

## Path

- `src/browser/cdp.rs`
- `src/browser/mouse.rs`
- `src/browser/session.rs`
- `src/cli/`
- `src/mcp/`
- `tests/`
- `docs/cli.md`
- `docs/mcp.md`

## Requirements

- Add hover, drag, key down/up/press, shortcuts, clear, check/uncheck, select,
  and file upload.
- Preserve separate human and fast semantics without stealth claims.
- Emit browser-faithful keyboard events where insertion alone is insufficient.
- Revalidate targets immediately before side effects.
- Keep upload paths subject to policy and never echo file contents.

## Verification

- Real input-event ordering and form-state tests across target/frame contexts.
- Drag, shortcut, contenteditable, autocomplete, select, checkbox, and upload
  scenarios.
- No wrong-target actions and bounded action evidence.
