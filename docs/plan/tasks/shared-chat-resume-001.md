---
id: shared-chat-resume-001
scope: Core, Glass, TUI, and Connect WebUI
status: in-progress
depends-on: []
---

# Shared conversation resumption and creation origin

## Objective

Implement and verify the complete contract in the shared-chat-resumption design.
Support both directions and WebUI, preserving the same log, ID, creation origin,
history, workspace, and single-writer protection. Include existing conversations;
do not limit this feature to newly created chats.

## Context

- [Design](../analysis/shared-chat-resumption.md)
- [Core runtime](../../../products/glass-core/docs/reference/runtime.md)
- [Glass runtime](../../development-runtime.md)
- [Glass TUI](../../architecture/development-tui.md)
- [Exact session recovery](../analysis/issue63-core-session-resume.md)

The main branch already contains the false-approval status fix in `c590a537`.
Preserve it and the bounded 50-entry Glass host catalog. The running gateway and
desktop must not be restarted during development or verification.

## Path

Core session, metadata, discovery, runtime, CLI/TUI, and Connect session modules;
Glass agent registry, durable thread references, runtime adapter, TUI pickers;
`products/glass-core/web/src` and generated production assets; focused regression
scripts and current guides. Update docs/INDEX.md and regenerate the documentation
catalog for documentation changes. Keep user untracked files intact.

## Verification

- Deterministic metadata/origin, canonical-reference, writer-lock, ownership, and
  invalid/mismatched-workspace tests.
- Real GC → Glass → GC and Glass → WebUI → Glass append/resume journeys with a local
  provider fixture. Same ID and canonical file; no transcript duplication or loss.
- WebUI browser interaction through the actual backend, including origin badges,
  resume, active-in-other-app behavior, and duplicate sidebar protection.
- Restart retains origin, source reference, and prior history; canceled/failed
  resumption preserves the current conversation and composer.
- Relevant package checks, TUI frames at compact/wide widths, web build/checks,
  documentation validators, and installed binary/assets verification.

Report concrete evidence and remaining blockers. Commit focused Conventional
Commits. Do not install binaries or restart live services; the orchestrator handles
activation after independent verification.
