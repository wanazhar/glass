---
id: native-engine-browser-000
scope: glass-browser/native-engine/core-web-profile
status: done
depends-on: [native-engine-234]
---

# BE-00: freeze the Glass Core Web Profile

## Objective

Create the versioned `GCWP-0.1` contract that governs the browser-complete
expansion of issue #40. The contract must cover external navigation, web
platform ownership, Glass operation parity, security, supported platforms,
conformance thresholds, performance budgets, exclusions, and revision policy.

This task freezes scope only. It does not claim that the current native engine
implements any newly listed capability.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue [#40](https://github.com/wanazhar/glass/issues/40)

## Contract

- `GCWP-0.1` is the source of truth for the first browser-complete promotion
  boundary.
- Native-only mode must not use Chromium/CDP or silently fall back.
- The profile is broad enough for ordinary external modern web pages and all
  normal Glass automation surfaces, while explicitly excluding proprietary and
  unstable features.
- Conformance requires mandatory-core 100% and selected-stable-suite 98%
  thresholds, zero unexplained failures, cross-platform evidence, and a
  native-only process/socket proof.
- Exactly two installable crates remain a workspace invariant; helper
  processes/modules do not create a third product crate.

## Paths

- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-000.md`
- `docs/plan/README.md`
- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- issue #40 body and canonical records

## Verification

- Check all new documentation links and profile/task references.
- Run `git diff --check`.
- Do not run Cargo: this task changes no Rust source, dependency, feature,
  build configuration, or generated artifact.
- Before any implementation task starts, require it to reference this profile
  and declare the exact `GCWP-0.1` capability family it advances.

## Completion evidence

The profile and task record are checked in locally. This task does not claim
WPT, real-site, browser-parity, security-boundary, performance, remote-CI, or
production evidence; those belong to BE-01 through BE-09 and M1 through M9.
