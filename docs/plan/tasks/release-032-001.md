---
id: release-032-001
scope: 0.3.2 release candidate
status: pending
depends-on: [development-032-003]
---

# Objective

Prepare the local 0.3.2 release candidate: package boundary for the umbrella
`glass-dev` command, synchronized metadata, release documentation, changelog,
package dry runs, complete validation, and a conventional local checkpoint.

# Context

- `docs/plan/analysis/release-032.md`
- `docs/release-checklist.md`
- `docs/release-evidence.md`
- `CHANGELOG.md`
- `.github/workflows/`

# Path

- `Cargo.toml`
- `Cargo.lock`
- `crates/glass-dev/`
- `README.md`
- `CHANGELOG.md`
- `docs/`
- `scripts/`
- `.github/workflows/`

# Verification

- version and documentation validators;
- formatting, locked tests, Clippy, rustdoc, deny/audit where installed;
- `cargo package --locked --no-verify` for each publishable package;
- `cargo publish --locked --dry-run --no-verify` for each package;
- release candidate report records unavailable platform/browser evidence;
- commit locally with a conventional `chore(release): prepare 0.3.2`
  checkpoint; do not push or publish.
