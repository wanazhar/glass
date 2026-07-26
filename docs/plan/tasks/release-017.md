---
id: release-017
scope: production and supply-chain hardening
status: completed
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

- The supported Linux x86-64 and macOS x86-64/arm64 release matrix passes real
  browser workflows. Windows is outside the published release contract.
- Corrupt/interrupted downloads never become executable installations.
- Crash cleanup and parser fuzz targets run in CI with documented budgets.
- Release artifacts reproduce documented version, help, and checksums.

## Accepted contract

Managed Chrome installation is a transaction: download a pinned version into a
new staging directory, stream to a bounded archive while hashing, compare the
release-pinned storage digest and length, extract with an in-process ZIP reader, validate the
platform executable, then atomically rename staging into the versioned install
directory. A stable `current` record changes only after validation. Startup
removes abandoned staging directories; `install-chromium --update` is the only
operation that changes an already valid managed version.

Disposable profile directories carry an owner record containing Glass PID and
process-start identity. Startup removes only records whose owner is provably
dead, never an active or malformed directory. Normal shutdown remains eager.

Parser fuzzing is split into bounded cargo-fuzz targets for MCP framing, CDP
messages, accessibility/DOM projection, locators, and URL policy. CI runs a
short deterministic corpus on every change and scheduled longer budgets.

Release CI builds each claimed target from a tag, runs platform-native browser
and archive smoke tests, emits the binary plus README/license, produces SHA-256
checksums, and signs the checksum manifest through keyless provenance. License,
dependency, and vulnerability reports are attached to the same workflow; a
failed report blocks publication rather than producing a partial release.

## Result

- Managed Chrome uses a release-pinned version, size, and SHA-256 digest; the
  bounded in-process extractor supports contained macOS framework symlinks and
  publishes through crash-durable monotonic generations.
- Disposable profiles use PID plus process-start ownership, eager cleanup, and
  bounded-batch crash recovery verified by a forced-process-exit test.
- Five production-parser fuzz targets run bounded pull-request and scheduled
  budgets without committing generated fuzz artifacts.
- Tagged CI fails closed across Linux x86-64 and macOS x86-64/arm64, then
  publishes packaged binaries, flat signed SHA-256 checksums, provenance, and
  blocking dependency/license/vulnerability reports. Windows remains outside
  the published release contract.
