---
id: observe-013
scope: consistent agent observation
status: in-progress
depends-on: [topology-011]
---

# Make compact observations consistent and frame-aware

## Objective

Produce a bounded observation tied to one target/frame topology generation and
make incompleteness explicit without turning full DOM or screenshots into
default costs.

## Context

- `docs/architecture/automation.md`
- `docs/architecture/browser.md`

## Path

- `src/browser/dom.rs`
- `src/browser/session.rs`
- `src/browser/cdp.rs`
- `tests/`
- `docs/architecture/browser.md`

## Requirements

- Detect mutations during collection and retry once within a deadline or mark
  the result inconsistent.
- Include explicit target/frame identity and truncation reasons.
- Cover shadow-root and frame boundaries with bounded summaries.
- Avoid collecting complete page text before truncation when a lower-allocation
  CDP path is proven correct.
- Retain compact-only caching with clear byte ownership.

## Verification

- Mutation-race, shadow DOM, frame, canvas/incomplete, huge-page, Unicode, and
  cache invalidation tests.
- Allocation/RSS and context-size gates on adversarial large pages.
- References from consistent observations remain actionable.
