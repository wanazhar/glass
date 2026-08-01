# Changelog

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and intends to use [Semantic Versioning](https://semver.org/).

## [Unreleased] — 0.2.4
- Browser-free MCP Web IR revision tools now map to canonical `webIr.diff` and
  `webIr.continuity` operations with typed Rust payloads and bounded results.
- MCP exposes browser-free `inspectWebIr`, `validateWebIr`, `diffWebIr`, and
  `continuityWebIr` tools for bounded draft inspection and revision analysis
  without starting Chrome.
- Browser-free MCP inspection and validation tools now map to canonical
  `webIr.inspect` and `webIr.validate` operations with typed draft payloads and
  bounded results.
- Browser-free MCP Task Protocol tools now map `compileTask` and `validateTask`
  to canonical `task.compile` and `task.validate` operations with typed task
  payloads and validation results.
- MCP Task Protocol dispatch now routes through the typed canonical request
  helpers while excluding MCP-only transport options from task payloads.
- MCP Web IR inspection, validation, diff, and continuity dispatch now routes
  through typed canonical protocol helpers while preserving bounded validation
  errors.
- Golden protocol fixtures now include typed successful Web IR and Task
  Protocol response envelopes with decoder round-trip coverage.
- Golden protocol fixtures now cover bounded typed preflight errors for Task
  Protocol and Web IR validation.
- Capability negotiation now advertises canonical `task` and `webIr` schema
  version `1` entries for browser-free protocol clients.
- Capability manifests now identify browser-free `taskProtocol` and draft
  `webIr` surfaces as `availableUncertified`.
- MCP Task tools now expose typed `taskValidation` and `taskCompilation` error
  details for invalid browser-free requests.
- Canonical protocol fixtures now cover typed `taskCompilation` preflight
  errors alongside Task validation and Web IR errors.
- CLI Task validation and compilation now route through the same typed
  canonical protocol helpers as MCP while preserving bounded output shapes.

## [0.2.3] - 2026-08-01

### Added

- Browser-free Web IR CLI tooling now supports validation, bounded inspection,
  deterministic canonicalization, revision diffs, and entity continuity
  classification.
- Rust crate-root exports expose the experimental extraction and draft Web IR
  contracts without making native browser extraction a stable API.
- Browser-free Task Protocol validation and compilation are available through
  the CLI and MCP, including bounded explanations and typed failures.

## [0.2.2] - 2026-07-30

### Added

- Effective capability negotiation now returns agreed protocol schemas and
  explicitly requested capabilities as a separate connection contract.
- Capability requests support required and optional capabilities, experimental
  opt-in, protocol-version lists, and additive request fields.
- Capability discovery responses tolerate additive fields and unknown future
  status values without accepting them for execution.

### Changed

- CLI and MCP agent-facing responses support `minimal`, `normal`, and
  `diagnostic` projections with bounded local result artifacts and explicit
  detail availability.
- MCP failures now carry stable recovery fields for phase, mutation possibility,
  retry classification, and recommended operation across protocol errors.
- Additive fields in semantic, intent, knowledge, and snapshot responses are
  tolerated while authored requests and policy/configuration contracts remain
  strict.

### Added

- Installation diagnostics report browser version, writable runtime paths, and
  MCP initialization health with actionable finding codes.
- Bounded `inspectPage`, `findTarget`, `actAndVerify`, `extractStructured`, and
  `recoverRun` operations are available through CLI and MCP.
- Five compile-tested workflow starter templates and five Rust quick-start
  examples.
- TypeScript and Python clients expose structured Glass errors while remaining
  experimental repository clients.

## [0.2.1] - 2026-07-30

### Added

- Explicit capability status reporting distinguishes available, policy-disabled,
  uncertified, and security-gated surfaces across CLI and MCP manifests.
- Experimental extension opt-in is explicit, sandbox-required, and limited to
  the locally verified Linux ARM64 environment.
- Deterministic reliability, knowledge migration, native sandbox, and client
  compatibility checks for the source checkout.

### Changed

- Release validation now checks the crates.io package shape and keeps native
  platform support claims separate from the locally verified host.

### Documentation

- Correct the published 0.2.0 release state and record the initial 0.2.1
  certification baseline.

## [0.2.0] - 2026-07-28

### Added

- A versioned semantic observation contract with bounded page classification,
  landmark regions, interactive target grouping, structured levels, scoped
  region expansion, and compact accessibility projections.
- Revision-aware semantic diffs with conservative continuity hints that never
  replace guarded action references.
- Semantic observation surfaces for the Rust library, CLI, MCP, TUI,
  TypeScript client, and Python client, plus deterministic fixture canaries and
  an offline scorecard.
- Bounded semantic intent resolution with candidate evidence, confidence
  policies, stale-revision handling, guarded execution, and semantic workflow
  steps across the library, CLI, MCP, TUI, TypeScript client, and Python client.
- Bounded profile-scoped persistent knowledge records with atomic local storage,
  lifecycle states, pruning, corruption handling, privacy checks, CLI/MCP
  management, a TUI inspector, and deterministic offline fixtures.
- Fresh-only and assessed semantic knowledge observations, plus explicit
  knowledge-backed intent resolution using historical fingerprints only as
  secondary evidence.
- YAML/JSON workflow authoring with canonical compilation, source diagnostics,
  safe parameter inference, redacted preview, stable diff output, and
  migration guidance.
- Semantic recorder drafts that retain bounded resolution evidence without
  storing replay handles or literal input values.
- Versioned reliability scenarios and fixture manifests with typed fault
  controls, independent side-effect oracles, redacted replay bundles, and a
  fail-closed certification gate for Linux and macOS evidence.

### Changed

- Documentation now separates the semantic observation guide from the compact
  legacy observation and keeps deep DOM, screenshots, and form values explicit.

## [0.1.18] - 2026-07-27

### Added

- Revision guards for all supported targeted mutations across the Rust, CLI,
  MCP, TypeScript, and Python interfaces.
- Bounded verification predicates, execution identities, effect witnesses,
  explicit recovery strategies, and revision-aware batch modes.
- Deterministic fixture coverage, MCP contract resources, and TUI action
  evidence for Linux and macOS workflows.

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
