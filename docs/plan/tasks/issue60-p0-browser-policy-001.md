---
id: issue60-p0-browser-policy-001
scope: glass-browser/resident-browser-policy
status: done
depends-on: [issue60-p0-governance-001]
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

- CLI dispatch now forwards the complete `BrowserPolicy` to MCP and TUI;
  workspace constructors enforce matching canonical roots and preserve exact
  host rules in the resident worker.
- The native resident runtime checks navigation URLs against that policy and
  refuses hardened/untrusted presets because it cannot provide Chromium's
  network interception guarantees. Resident responses include a top-level
  `browserRevision`, which the TUI projection consumes.
- Chrome resolver rules precede its positional URL. Interception lag is
  tracked separately from the last concrete policy denial.
- Focused regressions passed:
  - `cargo check -p glass-dev --lib --bins --locked`
  - `cargo test -p glass-dev --lib --locked browser_policy`
  - `cargo test -p glass-dev --lib --locked shared_workspace_forwards_exact_browser_host_rules_to_resident_service`
  - `cargo test -p glass-dev --lib --locked resident_browser_defaults_to_native_runtime`
  - `cargo test -p glass-browser --lib --locked hardened_navigation_intercepts_private_redirects_before_following`
  - `cargo test -p glass-browser --lib --locked hardened_host_resolver_rules_precede_the_positional_url`
  - `cargo test -p glass-browser --lib --locked policy_from_cli_preserves_exact_host_rules_and_preset`
  - `cargo fmt --all -- --check`
  - `git diff --check`
- Independent review: PASS; the reviewer confirmed the actual lagged receiver
  error is handled without replacing the blocked-request denial.
