---
id: issue60-p0-browser-policy-001
scope: glass-browser/resident-browser-policy
status: ready
depends-on: []
---

# Issue #60 P0: resident browser policy and denial integrity

## Objective

Resolve or reproduce findings F40/F48, F72, and F81. Carry one complete
revision and browser-policy contract through CLI/MCP setup, resident TUI
sessions, BrowserService, and Chrome launch. Put hardened resolver rules before
the positional URL. Ensure event-stream lag cannot overwrite a real
`BlockedByClient` denial.

## Context

- `docs/policy.md`
- `docs/architecture/browser.md`
- `docs/architecture/development-tui.md`
- `docs/plan/analysis/issue-60.md`
- [Issue #60](https://github.com/wanazhar/glass/issues/60), findings F40/F48/F72/F81
- Source reports [#47](https://github.com/wanazhar/glass/issues/47), [#49](https://github.com/wanazhar/glass/issues/49), and [#50](https://github.com/wanazhar/glass/issues/50)

## Contract

- Every resident and persistent browser entry point receives the same policy
  preset, host allow/deny rules, and revision shape as the direct CLI path.
- All Chrome policy flags precede positional arguments.
- Interception lag is reported separately and never replaces an actionable
  policy denial.
- Keep host checks fail-closed and preserve current hardened-policy behavior.

## Path

- `crates/glass-dev/src/browser.rs`
- `crates/glass-dev/src/lib.rs`
- `crates/glass-browser/src/browser/chrome.rs`
- `crates/glass-browser/src/browser/session/mod.rs`
- Relevant CLI/TUI browser launch code and policy documentation

## Verification

- Add launch-argument ordering coverage for hardened host rules.
- Add an interception regression where a real denial is followed by a lagged
  event receiver; both the denial and lag signal must remain observable.
- Exercise or reproduce policy forwarding through the resident browser path,
  including exact host allow and deny behavior and revision projection.
