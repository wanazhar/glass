id: mobile-onboarding-036-004
scope: phone UX and discovery
status: complete
depends-on: [product-ux-036-003]

## Objective

Implement purpose-built Agent/Code/App/Tasks/More phone navigation, useful
startup detection, and a searchable editable command palette.

## Context

- `docs/architecture/product-workspace.md`
- `docs/architecture/browser-workspace.md`
- `docs/architecture/mobile-cockpit.md`

## Path

- `crates/glass-dev/src/tui/`
- onboarding/readiness adapters

## Verification

Scripted 48x18, 64x24, and 80x24 flows prove important content remains
reachable through trust, Agent, App semantics, Code, Tasks, More and palette.
