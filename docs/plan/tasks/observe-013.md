---
id: observe-013
scope: consistent agent observation
status: done
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

## Completion

- Compact observations now snapshot one immutable target/frame route, collect
  inside an isolated execution world, retry one detected mutation race within
  a one-second attempt deadline, and explicitly publish both CDP and DOM
  revision evidence.
- Visible text is UTF-8 bounded to 16 KiB before it crosses CDP. Accessibility
  nodes, labels, and controls have distinct truncation reasons; shadow roots,
  child frames, canvases, and the 512-element boundary scan remain bounded and
  explicit.
- Only consistent compact contexts are cached. Full DOM and screenshots remain
  named, opt-in operations, and selected-frame observations retain actionable
  revisioned references.
- Unit and real-Chrome coverage exercises mutation races and recovery, cache
  invalidation, shadow/frame/canvas boundaries, large Unicode content, a
  sub-32-KiB compact context gate, and a sub-64-MiB Glass RSS delta gate.
- A five-iteration release sanity run measured cached observation at 0.019 ms
  p50, fresh observation at 9.53 ms p50, a 21,123-byte fixture context, and
  7,999,488 bytes of post-workload Glass RSS.

## Commit

`feat: add consistent bounded observations`
