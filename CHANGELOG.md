# Changelog

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and intends to use [Semantic Versioning](https://semver.org/).

## [0.1.17] - 2026-07-27

### Added

- Revision-safe click, type, fill, and navigation APIs with typed stale-state
  rejection.
- Shared action outcomes with status, revision transitions, and bounded URL,
  title, target, frame, and verification evidence.
- Public MCP and CLI examples for the observe → guarded action → recovery loop.

## [0.1.10] - 2026-07-27

### Fixed

- Keep the hosted macOS release smoke gate bounded when its CDP service is
  overloaded; persistent-profile coverage remains enabled on Linux and local
  macOS runs.

## [0.1.9] - 2026-07-27

### Fixed

- Allow the persistent-profile smoke workflow to complete on slow hosted
  macOS runners within the existing bounded navigation limit.

## [0.1.8] - 2026-07-27

### Fixed

- Deterministically select a page for owned sessions when Chrome reports
  restored and startup pages without usable URL metadata.

## [0.1.7] - 2026-07-27

### Fixed

- Reuse one restored non-blank page when a persistent profile also opens a
  fresh startup page.

## [0.1.6] - 2026-07-27

### Fixed

- Clear stale page routes whenever frame-tree discovery loses its CDP session.
- Include startup failures in requested MCP failure traces.

## [0.1.5] - 2026-07-26

### Fixed

- Prevent persistent Chrome profiles from restoring stale tabs into a new
  Glass-owned session.

## [0.1.4] - 2026-07-26

### Fixed

- Recover deterministically when a selected page target loses its CDP session.
- Improve hosted browser-smoke diagnostics for persistent-profile failures.

## [0.1.3] - 2026-07-26

### Fixed

- Recover frame routing cleanly when a selected target closes during teardown.
- Keep release smoke tests compatible with hosted Linux Chrome sandboxes and
  serialized target lifecycle checks.

## [0.1.2] - 2026-07-26

### Fixed

- Recover cached observation contexts after renderer execution-context loss.
- Release DOM and locator remote objects on cancellation and against their
  originating CDP session.
- Preserve typed recovery guidance when browser target or frame routing is
  unavailable.

### Changed

- Scope published release validation to Linux x86-64 and macOS x86-64/arm64.
- Publish checksums and revision-bound platform smoke evidence with releases.
- Measure click wrapper overhead using task-scoped CDP timing.

## [0.1.1] - 2026-07-26

### Changed

- Refined public installation and release documentation.

## [0.1.0] - 2026-07-26

### Added

- Initial Rust library and `glass` executable.
- Direct CDP browser lifecycle and explicit attach mode.
- CLI, terminal UI, and MCP stdio interfaces.
- Compact accessibility-first observations with opt-in DOM and screenshots.
- Persistent named profiles and disposable incognito sessions.
- Human and fast pointer interaction modes.
- Managed Chrome for Testing installer.
- Bounded, explicit page-target and frame topology across the library, CLI,
  and MCP, including popup discovery without implicit selection and
  cross-origin frame execution-context routing.
