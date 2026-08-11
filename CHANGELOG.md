# Changelog

This project follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and intends to use [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Added identity-bound repository trust, autonomous verified task DAGs,
  direct Pi SDK sessions, bounded daemon event cursors, governed kernel tool
  bindings, measured experiments, broader DAP evidence, and complete resident
  TUI surfaces.

### Changed

- Moved the executable development core into `glass-dev` and removed the
  `glass-browser/development-runtime` feature bridge.
- Replaced generated GitHub notes with validated substantive release bodies
  bound to exact tag, commit, workflow, migration, limitations, and evidence.

### Security

- Repository-controlled configuration cannot execute or elevate authority
  before an explicit local trust decision; `--yolo` does not bypass trust.
- Release publication now blocks unless GitHub itself verifies the signed tag.

## [0.3.4] - 2026-08-10

### Added

- Added the Glass-owned resident Development Workspace with shared editor,
  PTY, Git, test, LSP, DAP, execution-kernel, browser, semantic-memory,
  workflow, graph, replay, experiment, and customization services.
- Added native Pi session controls, independent concurrent Pi agents with
  dependency scheduling and cancellation, bounded evidence, model/thinking
  controls, persistent sessions, and durable-daemon tool brokerage.
- Added real DAP framing and lifecycle support, including launch/attach,
  configuration sequencing, breakpoints and exception filters, stepping,
  threads, stacks, scopes, variables, evaluation, restart, disconnect, and
  termination. A live debugpy scenario exercises breakpoint-to-continue state.
- Added a complete shared LSP manager with lifecycle, diagnostics, completion,
  navigation, symbols, signature help, code actions, formatting, semantic
  tokens, rename, attribution events, and a bounded raw request escape hatch.
- Added native Git/worktree operations, structured test discovery and runs,
  affected/watch/cancel flows, and persistent Python, JavaScript, shell, and
  SQLite kernels.
- Added isolated competing experiments and evidence-derived ranking, plus a
  typed causal Development Graph and observable replay/diff timeline.
- Added strict `glass.toml` customization for Pi skills, agent defaults,
  configured LSP/DAP/test services, bounded hooks, commands, and governed
  schema-validated custom tools.

### Changed

- `glass-dev` now owns the full development TUI and resident service registry;
  `glass-browser` remains the one-way browser-intelligence dependency and
  standalone browser product.
- `glass --mcp` merges the browser catalog with every available resident Glass
  Dev tool. External CLI and MCP callers use the same actor-attributed,
  revision-checked router as Pi and the TUI without being nested through Pi.
- The native TUI is decomposed into state, command, and rendering modules with
  desktop, compact, and phone layouts and command-palette access to editor,
  agents, processes, LSP, DAP, Git, tests, kernels, experiments, graph, replay,
  browser, and workflow controls.
- `glassd` now owns complete Development Workspaces across client disconnects,
  retaining Pi sessions, processes, browser identity, language/debug services,
  kernels, tests, graph, replay, and configuration under scoped local IPC.

### Fixed

- Diagnose Chrome status 21 caused by confined Snap profile locks and provide
  actionable incognito, managed-Chromium, and unconfined-browser guidance.
- Automatically place persistent profiles for detected Snap Chromium in its
  accessible common-data directory, preserving zero-configuration startup on
  Linux ARM64 without `--chrome-path`, `--incognito`, or manual CDP attachment.
- Close resident Chromium sessions when their worker channel disconnects so
  headless browser processes and disposable profiles cannot become orphans.
- Sequence DAP launch/attach with `configurationDone`, matching real adapters
  such as debugpy that defer startup responses until configuration completes.

## [0.3.3] - 2026-08-10

### Added

- Added independent layout, transport, graphics, shell, and multiplexer
  classification with conservative unknown states, measured overrides, typed
  presentation profiles, policy reasons, and requested/acquired/presented
  observability.
- Added a six-view phone cockpit (Overview, Agent, Browser, Project, Diff, and
  Process), printable navigation, command-palette hints, browser recovery, and
  privacy-preserving target selection.
- Added loopback-only, random-token Remote View for the active BrowserSession,
  newest-frame delivery, bounded clients/input, revision-guarded browser input,
  SSH-forward guidance, and explicit revocation.
- Added explicit project-tree truncation/ignore evidence, checked process-state
  reads, process-group shutdown escalation, persistent LSP document lifecycle
  and language operations, and a real Neovim `--embed` Msgpack-RPC proof.
- Added attached-agent browser/project revision context and Linux, macOS, and
  Windows browser-free CI coverage.
- Added native phone-cockpit mouse/touch hit testing, searchable actions,
  revision-bound semantic taps, command history, approvals, and visible agent
  progress without requiring function keys.
- Expanded the Pi coding harness with workspace-confined read, search, edit,
  file lifecycle, process, command, diagnostic, semantic, Web IR, and task
  tools; exact per-call mutation approvals; bounded audit evidence; and a
  Glass-specific system prompt.
- Added explicit `glass --yolo` operation for trusted local automation with
  unrestricted Pi tool/resource availability and no Glass or Pi approval
  prompts, while keeping the mode visibly persistent in the cockpit.
- Added `glass update` and `glass-browser update`, which resolve the owning
  Cargo package and install root, verify source provenance, preserve the
  full/core package boundary, support dry-run/version/force/registry choices,
  and avoid Windows in-process executable replacement.

### Changed

- Restored local interactive targets to 30 FPS balanced and 60 FPS smooth;
  adaptive mode reduces resolution before rate and aggressively throttles
  settled, idle, and background work.
- Stopped inferring graphics support or phone layout from terminal/SSH names.
  Mosh now remains semantic-only and unknown remote links fail closed.
- `cargo install glass-dev` now installs both `glass` and `glass-browser` while
  `cargo install glass-browser` remains the independent core installation.
- Browser startup failure no longer terminates the TUI worker; free,
  compatible, unrelated, and unknown endpoints produce explicit recovery
  choices without silently attaching.
- Reduced remote redraw, allocation, and terminal-write pressure while keeping
  local interaction responsive, and split workspace validation by package to
  avoid Cargo's duplicate binary-target warning without losing coverage.
- Reworked the complete product, installation, uninstallation, CLI, runtime,
  mobile, architecture, security, SDK, and troubleshooting documentation.

### Fixed

- Fixed the corrupt iOS design artifact, silent 2,048-entry project-tree
  truncation, discarded PTY polling errors, child-only Unix termination,
  repeated one-shot LSP startup, and the false headless-Lua Neovim RPC claim.
- Replaced allocation-heavy workspace snapshot fingerprints with a persisted,
  monotonic revision so concurrent stale saves fail closed deterministically.
- Drained bounded PTY output before reporting process completion, moved the
  complete CLI lifecycle onto a larger Windows stack, routed bounded Windows
  commands through owned non-interactive pipes, released completed ConPTY
  masters before draining output, and removed platform-only Cargo warnings
  from browser-free builds.
- Made transient workspace operations release ownership locks explicitly so
  immediate same-process saves, reopens, and deletes cannot race lock teardown.
- Kept browser-only builds usable for file, search, and semantic development
  operations without PTY support, while gating real Neovim and command probes
  on the `development-runtime` feature.
- Corrected Windows workspace replacement and durability semantics, lock and
  refused-connection classification, portable source paths, ConPTY startup,
  Git worktree paths, and Remote View HTTP shutdown behavior.

## [0.3.2] - 2026-08-08

### Added

- Added a single-pane mobile TUI with phone-safe navigation, automatic
  SSH/Mosh breakpoints, Herdr context detection, semantic-only remote browser
  presentation, and private Safari port-forwarding guidance.
- Added a bounded local development runtime for project detection, optional
  `glass.toml` configuration, workspace-confined files and native buffers,
  atomic saves, PTY processes, actor-attributed events, timelines, runtime
  links, and cross-surface diffs.
- Added `glass project` CLI commands and browser-free MCP tools for project
  inspection, file editing, process control, runtime links, diffs, and event
  timelines.
- Added a responsive TUI Development workspace with a native editor, live app,
  files/Git, runtime/tests/actors, attributed timeline, asynchronous LSP work,
  and a non-blocking long-lived Pi RPC worker.
- Added real rust-analyzer diagnostics, PTY health and URL detection, explicit
  source/runtime graph links, semantic breakpoints, replay, worktree
  experiments, collaboration claims, global search, and Neovim PTY/RPC probes.
- Split distribution into the `glass-browser` crate and `glass-browser`
  executable plus the exact-version `glass-dev` package that owns `glass`.
- Added executable Chromium goldens for the Web IR corpus, an adversarial and
  metamorphic semantic suite, relationship-scoped task compilation,
  revision-bound live bindings, runtime capability declarations,
  `entityState` verification, compact Web IR projections, and value-free
  execution receipts.
- Added a shared schema-validating Local/Pi tool gateway with explicit mutation
  authority, bounded outputs, metadata-only audit events, and read-only Web IR
  inspection, diff, continuity, and task-planning tools for Pi.
- Added a remote development cockpit with bounded resident project sessions,
  persistent MCP PTYs, non-sensitive reconnect capsules, a mobile attention
  inbox, opt-in terminal alerts, revision-bound semantic tap actions, compact
  verification cards, adaptive live quality, and higher-level TypeScript and
  Python orchestration workflows.
- Added complete typed Development Runtime helpers to the repository
  TypeScript and Python clients, plus a negotiated, cursor-bounded project
  event feed that reports timeline-compaction gaps without streaming pixels
  through MCP.
- Added an explicit terminal-native live browser for remote and phone TUI
  sessions with Herdr-owned PNG streaming, direct Kitty capability probing,
  true-color ANSI half-block fallback, adaptive data/balanced/smooth profiles,
  fit controls, runtime diagnostics, and a browser-free renderer benchmark.

### Changed

- Disabled continuous visual capture in phone mode while preserving explicit
  screenshots and the structured browser observation contract.
- Kept structured browser observation, explicit screenshot escalation, browser
  policy, MCP framing, and existing v0.3.1 browser contracts unchanged.
- Require live browser revision evidence before confirming a build/live-update
  event, and keep inferred framework source maps explicitly uncertain.
- Reorganized public documentation by user role and added complete feature,
  CLI-family, MCP-tool, Rust SDK, runnable-example, package, and docs.rs entry
  references backed by generated inventory and local-link validation.
- Reduced maximum-width semantic form compilation by indexing graph scope and
  normalized field names once, removed per-binding candidate allocations, and
  retained the embedded-agent tool catalog for gateway lifetime.
- Enforced open-handle byte limits for Pi broker, project, editor, graph, and
  language-server file reads; tool JSON evidence and result sizing now stream
  without duplicate payload buffers.
- Propagated checked and disabled control state into Web IR, removed mutation
  actions from disabled/read-only entities, classified sensitive fields
  conservatively with token-aware rules, and scoped continuity by graph context.
- Bumped deterministic task compilation to compiler version 2; version-1 plans
  must be recompiled before execution. Live execution now uses scoped semantic
  bindings throughout, and nested form ancestry is preserved through labels.
- Moved Pi semantic-tool calls across the process boundary through private,
  size-bounded, one-use request files instead of process arguments.
- Kept structured browser semantics and explicit screenshots as the default;
  live pixels now degrade from Herdr to Kitty to ANSI to semantics, select ANSI
  automatically under Mosh, and retain Safari port forwarding as the stable
  full-fidelity iPhone path.

## [0.3.1] - 2026-08-07

### Added

- Added profile-, workspace-, and origin-scoped semantic memory with exact and
  graph retrieval, optional injected embeddings, explicit lifecycle states,
  provenance, inspection, export, pruning, and deletion. Memory remains
  advisory and must match fresh Web IR evidence before influencing compilation.
- Added a bounded multi-surface model for document, accessibility, frame,
  shadow, SVG, graphics, media, terminal, remote-application, browser-native,
  extension, and opaque surfaces with explicit coverage and action evidence.
- Added a transport-neutral browser capability boundary, deterministic backend
  selection, the production CDP adapter, a bounded experimental WebDriver BiDi
  backend, and a browser-free semantic proof backend for conformance tests.
- Added persistent workspace identity, resource references, actor roles,
  revision-guarded mutation leases, observer attachments, replay inspection,
  and semantic revision diffs across CLI, MCP, daemon, TUI, and Rust contracts.
- Added browser-presentation and terminal-graphics contracts with bounded
  latest-frame delivery, Kitty rendering, semantic fallback, viewport-safe
  input mapping, performance metrics, and takeover-aware TUI workspace modes.
- Added user-facing `workspace`, `memory`, `surfaces`, `backend`, `replay`, and
  `doctor` command families plus a useful command-free TUI entry point.

### Changed

- Routed live extraction, semantic actions, task execution, and presentation
  through revision-bound backend and surface evidence without exposing raw
  transport identifiers as public execution authority.
- Updated package metadata, installation guidance, migration documentation,
  capability matrices, and release checks for the integrated issue #31
  architecture.

### Fixed

- Hardened workspace persistence, profile ownership, frame lifecycle, surface
  provenance, backend dispatch, semantic-memory scoping, and advisory rejection
  against stale, cross-scope, malformed, or unsupported inputs.


## [0.3.0] - 2026-08-06
- Promoted bounded extraction reconciliation to stable Glass Web IR v1 with
  strict entity-detail invariants, canonical serialization, revision diffs,
  and fail-closed continuity classification.
- Added deterministic Task Protocol v1 compilation against validated Web IR,
  producing value-free guarded plans with evidence, risk, ambiguity,
  confirmation, revision, postcondition, and resource metadata.
- Routed every browser-backed task family through fresh Web IR extraction and
  compilation before dispatch, with compatible-revision handling, generated
  verification postconditions, and typed recovery.
- Standardized browser-free Web IR inspection, validation, diff, continuity,
  and task compilation across Rust, CLI, MCP, protocol, daemon, capability,
  and golden-conformance surfaces.
- Completed bounded accessibility, DOM, forms, navigation, dialogs, tables,
  collections, frame, shadow-root, and opaque-region evidence collection with
  explicit missing-source, truncation, coverage, and deadline semantics.

## [0.2.9] - 2026-08-06
- Generalized the browser-backed `executeTask` MCP contract across all
  validated Task Protocol families and documented family-specific revision
  guards.
- Hardened bounded pagination collection to await semantic transitions and
  fail closed when a successful click leaves a usable next control unchanged.
- Made structured-extraction `serializedBytes` account for the complete
  bounded response and enforce `maxBytes` against that payload.

## [0.2.8] - 2026-08-01
- Added explicit typed structured-extraction kinds, field-level semantic
  provenance, entity references, and observed output-limit metadata.
- Added bounded `recordItems` for semantic table and repeated-collection
  extraction while preserving the compatibility records envelope.
- Added bounded semantic table and collection records from accessibility
  evidence, with compatibility fallback to interactive targets.
- Structured record changes now participate in revision-aware semantic page
  change detection for task verification and pagination.
- Added bounded `startIndex` requests and revision-bound continuation metadata
  for resumable table and repeated-item extraction.
- Continuation requests now fail closed when source revision or route metadata
  does not match the fresh semantic observation.
- Continuations now retain and validate the requested semantic region scope.
- Continuations now retain and validate a non-sensitive extraction contract
  fingerprint.
- Added fail-closed sensitive extraction gating for secret-like field names
  and paths.
- Sensitive extraction denials now use the typed policy error contract with a
  stable `read_sensitive_extraction` operation.

## [0.2.7] - 2026-08-05
- Hardened media clicks for native `<video>` / `<audio>` targets, including
  state-aware Play/Pause fallback and idempotent already-satisfied controls.
- Raised compact observation's bounded per-flight deadline to three seconds
  and made wait probes recover the active frame route after SPA navigation.
- Added bounded `smoke-sites` live-site compatibility probing with isolated
  sessions, structured observation metrics, safe target preflight, revision
  continuity, and policy-aware classifications.
- Improved `batch` input diagnostics for stdin, file paths, and rejected inline
  JSON; CI navigation now permits public HTTP(S) URLs without host pinning
  while preserving explicit host deny and allow rules.
- Added viewport-aware semantic observations with bounded document text,
  document-style page classification, and explicit truncation metadata.
- Persisted revision identity across attached CLI/MCP sessions and enriched
  wait timeouts with the last observed page route when available.
- Added a browser-backed, revision-checked Task Protocol runtime for scoped
  `form.inspect`, `form.fill`, `form.validate`, and `form.submit` operations
  with bounded timeouts, confirmation gates, and indeterminate-outcome
  recovery states.
- Exposed verified form task execution through `glass task execute` and the
  MCP `executeTask` tool, both using the same guarded runtime and fresh
  postcondition observation.
- Added bounded `region.extract` execution through the same revision-guarded
  Task Protocol, CLI, and MCP surfaces with source revision and provenance.
- Standardized Task Protocol execution recovery as typed `retry` guidance with
  explicit classifications and recommended operations across CLI, MCP, and Rust
  results.
- Added revision-guarded `navigation.follow` execution with bounded URL
  inputs, shared CLI/MCP routing, verified revision output, and fail-closed
  recovery for indeterminate navigation.
- Added revision-guarded `navigation.selectTab` execution with unique semantic
  tab resolution, fresh post-action observation, and typed indeterminate
  recovery.
- Added guarded `dialog.inspect`, `dialog.confirm`, and `dialog.cancel`
  execution with pending-dialog checks, closure verification, confirmation
  policy, and typed indeterminate recovery.
- Added bounded `pagination.next` execution with semantic control resolution,
  fresh post-action observation, and typed indeterminate recovery.
- Added typed pending-dialog details to `dialog.inspect` task results without
  changing action-result recovery semantics.
- Added one canonical Rust task dispatcher shared by CLI and MCP browser-backed
  execution paths.
- Added bounded `collection.extract` execution for uniquely scoped semantic
  collection regions with explicit `$.targets` provenance.
- Added bounded `table.extract` execution for uniquely scoped semantic table
  regions with explicit `$.targets` provenance.
- Added guarded `field.read` execution with bounded form-state output and
  existing sensitive-value redaction.
- Added password redaction coverage and a post-observation revision guard to
  `field.read`.
- Added authored-task validation requiring `inputs.field` for `field.read`.
- Added authored-task validation requiring explicit semantic region scopes for
  browser-backed task families.
- Added bounded `pagination.collect` execution with revision-aware advances,
  explicit limits, and indeterminate recovery guidance.
- Hardened extraction against revision/route drift and stopped pagination
  collection on semantic no-op controls rather than revision bookkeeping.
- Added guarded `navigation.openMenu` execution with explicit semantic menu
  inputs, revision-aware clicking, and post-action observation.
- Hardened `navigation.openMenu` to require observable expanded state after
  clicking; unverified clicks now return indeterminate recovery guidance.
- Hardened `navigation.selectTab` to require delayed-safe observable
  `aria-selected` state and typed indeterminate recovery.
- Hardened `pagination.next` to require a bounded semantic page or route
  transition after clicking, with delayed-success and no-op recovery coverage.
- Hardened `navigation.follow` to verify the final destination URL and report
  redirect or mismatch outcomes as indeterminate recovery.
- Hardened `form.submit` target resolution to reject non-submit semantic
  controls during preflight before browser mutation.
- Hardened `form.fill` to return structured indeterminate recovery results for
  operation or post-fill inspection failures instead of leaking errors.
- Bound post-action observation failures for navigation, pagination, and form
  submission to structured indeterminate recovery results.
- Required explicit bounded postconditions for `form.submit` and made
  `NavigationOccurred` compare against the source revision.

## [0.2.6] - 2026-08-01
- GitHub Release coverage checks now tolerate release API propagation before
  declaring a version tag missing.

## [0.2.5] - 2026-08-01
- Web IR diff and continuity now reject revision regressions and same-revision
  content drift while preserving deterministic self-comparisons.
- Added issue #30 revision contract coverage for stale transitions, rebinding,
  removal, and ambiguous continuity.
- Release workflow now creates source-only GitHub Releases for version tags,
  marks the newest release as `Latest`, and validates that every version tag
  has a matching published release record.

## [0.2.4] - 2026-08-01
### Added

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
- CLI Web IR validation, inspection, and continuity now use typed canonical
  protocol helpers; detailed local diff output remains available.
- Canonical Web IR fixtures now cover typed preflight failures for diff and
  continuity operations.
- Web IR CLI diffs now support an explicit `--summary` mode for bounded
  canonical output while preserving detailed diagnostics by default.
- Task Protocol execution plans now validate bounded limits, operation
  alignment, fill-input metadata, and confirmation safety before runtime use.
- Compiled plans reuse Task Protocol bounds for postconditions and fill-input
  counts, preventing oversized response contracts from reaching runtimes.

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
  the Linux ARM64 validation environment.
- Deterministic reliability, knowledge migration, native sandbox, and client
  compatibility checks for the source checkout.

### Changed

- Release validation now checks the crates.io package shape and keeps native
  platform support claims separate from the recorded Linux ARM64 validation
  environment.

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
