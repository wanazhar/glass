---
id: lifecycle-002
scope: browser lifecycle
status: done
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

## Completion

- Owned sessions refuse an occupied CDP port; attachment is an explicit
  `--attach` mode with validated launch-only option conflicts.
- Target selection never adopts the first page silently when multiple targets
  exist; `--target-id` selects the intended Chrome page.
- Incognito launches both `--incognito` and a unique Glass-owned user-data
  directory, which is removed after the owned process stops.
- Chrome user-data directories are the named-profile authority; profile deletion
  also removes legacy Chrome directories and JSON metadata.
- Managed Chrome for Testing is resolved before system Chrome/Chromium.
