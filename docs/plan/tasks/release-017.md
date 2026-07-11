---
id: release-017
scope: production and supply-chain hardening
status: pending
depends-on: [mcp-008, policy-016, visual-015]
---

# Harden installation, crashes, parsers, and release artifacts

## Objective

Make every supported platform and distribution path verifiable, recoverable,
and resistant to malformed input or interrupted lifecycle operations.

## Context

- `docs/architecture/automation.md`
- `docs/installation.md`
- `docs/release-checklist.md`

## Path

- `.github/workflows/`
- `src/browser/chrome.rs`
- `src/browser/profile.rs`
- `src/mcp/`
- `tests/`
- `Cargo.toml`
- `docs/`

## Requirements

- Stream, pin, integrity-check, and atomically install managed Chrome; add an
  explicit update path.
- Avoid an undeclared external `unzip` dependency or package it per platform.
- Recover abandoned disposable profiles and test forced termination.
- Fuzz MCP, CDP response, AX, DOM, locator, and URL/policy parsers.
- Run real-browser CI and artifact smoke tests on every claimed platform.
- Produce signed checksums and dependency/license/security reports.

## Verification

- Linux, macOS, and Windows release matrix passes real browser workflows.
- Corrupt/interrupted downloads never become executable installations.
- Crash cleanup and parser fuzz targets run in CI with documented budgets.
- Release artifacts reproduce documented version, help, and checksums.
