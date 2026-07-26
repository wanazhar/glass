# Changelog

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and intends to use [Semantic Versioning](https://semver.org/).

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
