# Issue 63: negotiated mixed runtime tools

## Goal

Run Core-native model/session/provider/config/auth and coding tools together
with Glass-owned browser, Git, and project integrations in one resident session.
Preserve standalone Core behavior and explicit legacy Pi selection.

## Acceptance checklist

- [x] Publish a focused design analysis at
      `docs/plan/analysis/mixed-runtime-tools.md` before implementation.
- [x] Negotiate an explicit mixed-tools protocol/capability; reject unsupported
      runtimes clearly and preserve protocol v1 host/local semantics.
- [x] Keep provider credentials, provider selection, model configuration, and
      session authority in Core; do not duplicate credentials in Glass.
- [x] Register only Glass-owned integrations from Glass and route only those
      calls through Glass authorization/evidence.
- [x] Keep the mixed catalog narrow: omit direct editor writes, generic
      commands, process controls, and duplicate Core agent/model/session tools.
- [x] Execute Core-native coding tools inside Core in the same session.
- [x] Reject schema/name collisions and preserve correlation, results,
      progress, and cancellation for Glass calls.
- [x] Preserve the host-neutral Core standalone mode and explicit Pi legacy
      decision.
- [x] Update current docs, setup/install language, validators, and fixtures as
      needed without rewriting historical evidence.
- [x] Add behavioral tests for both owners and compatibility/error paths.
- [x] Run focused package tests, format and strict scoped Clippy; run
      documentation/version checks; inspect for unrelated changes.
- [x] Commit the implementation in focused Conventional Commits. Do not push,
      publish, tag, release, globally install, or interrupt existing services.

## Scope exclusions

Do not change TUI behavior, unrelated Glass source, browser engine behavior,
provider implementations, or the historical commit graph. Do not convert Core
into a Glass-dependent crate. Do not replace Core's coding tools with Glass
file tools or broaden Glass authority over Core-owned calls.
