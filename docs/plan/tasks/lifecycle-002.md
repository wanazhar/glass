---
id: lifecycle-002
scope: browser lifecycle
status: pending
depends-on: [perf-001]
---

# Explicit browser ownership and profile modes

## Objective

Correct incognito/profile behavior and prevent unintentional adoption of an existing CDP browser without adding a heavyweight lifecycle layer.

## Context

- `docs/architecture/README.md`
- `docs/architecture/browser.md`
- `docs/plan/analysis/performance-overhaul.md`

## Path

- `src/browser/chrome.rs`
- `src/browser/profile.rs`
- `src/browser/session.rs`
- `src/cli/args.rs`
- `src/cli/runner.rs`
- `tests/`
- `docs/architecture/browser.md`

## Requirements

- Add explicit attach mode; an occupied endpoint without attach must fail clearly.
- Implement real incognito with `--incognito`, a unique disposable user-data directory, and cleanup for Glass-owned sessions.
- Use Chrome user-data directories as profile persistence source of truth; deletion removes persisted browser data.
- Resolve Chrome installed by `install-chromium` before system detection.
- Require an explicit target ID when attachment is ambiguous.

## Verification

- Unit tests for profile/target option validation.
- Browser smoke coverage for incognito isolation and owned vs attached session behavior when feasible.
- Full fmt, Clippy, and all-target test suite.

## Commit

`fix: isolate browser profiles and attachment`
