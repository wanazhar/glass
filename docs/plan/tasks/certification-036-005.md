id: certification-036-005
scope: integration and release certification
status: complete
depends-on: [mobile-onboarding-036-004]

## Objective

Certify scenarios A-J, gates 1-15, and every forbidden outcome; synchronize
0.3.9 packages/docs/release automation and publish the corrected exact-tag product.

## Context

- `docs/plan/analysis/release-036.md`
- `docs/plan/reviews/release-036-gates.md`
- `docs/release-checklist.md`

## Path

- interaction, PTY, browser, daemon, lifecycle and release tests
- Cargo/package/client/version metadata
- public docs, changelog, release notes/evidence, and workflows

## Verification

All local release gates, both package and publish dry runs, isolated installs,
live browser scenarios, exact-tag remote CI, crates.io propagation, GitHub
Release verification and issue closure must pass.
