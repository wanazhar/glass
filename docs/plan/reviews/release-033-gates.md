# Glass v0.3.3 issue #33 gate review

Status: final local candidate review

This is the auditable mapping for all 53 mandatory checkboxes in issue #33,
including its authoritative amendment. A checked item means an integrated code
path, focused test or packaging/CI gate exists; it does not claim publication
or runtime certification on an unobserved platform.

## Gate 1 — local browser performance

- [x] Local active targets 60 FPS where supported — `connection::local_profiles_restore_sixty_and_thirty_fps` and the Smooth policy.
- [x] Normal local activity is not capped below 30 FPS — Balanced is 30 and Smooth is 60.
- [x] Idle/background throttling remains aggressive — `connection::activity_throttles_without_erasing_active_profile` proves idle at most 3 and background 0.
- [x] Stale queues stay bounded/latest-state — presentation and terminal payload mailbox replacement tests cap pending state.
- [x] Requested versus achieved FPS are distinguishable — `PresentationMetrics` reports requested, acquired and presented rates independently.

## Gate 2 — remote policy correctness

- [x] Layout, transport and graphics are independent — `ConnectionEnvironment` has separate typed dimensions and override evidence.
- [x] 3/6/12 profiles are constrained/assistive only — the remote matrix confines these targets to unknown/constrained transport.
- [x] Phone width alone does not imply slow transport — geometry-only layout tests retain the same transport classification.
- [x] Remote diagnostics explain the selected mode — policy reasons, measurements and evidence render in `live doctor`.
- [x] Mosh is not a generic graphics stream — Mosh auto-policy is semantic with zero continuous pixel FPS.

## Gate 3 — mobile UX

- [x] Phone layout is intentionally designed — stacked Overview/Agent/Browser/Project/Diff/Process cards replace the desktop grid.
- [x] Semantic, agent and process state works without pixels — phone semantic mode and browser-free project/agent/process commands are independent of capture.
- [x] Visual assist is explicit — `live on` or Remote View is required; screenshots remain explicit.
- [x] Critical actions work without function keys — printable `1`–`6`, `Tab`, `?`, `:`, commands, `Enter` and `Esc` are tested.
- [x] Design references are checked against real terminal output — the 40×20 PTY smoke captures the executable's ANSI phone render; deterministic cell snapshots cover adjacent states.

## Gate 4 — one browser reality

- [x] TUI, Remote View, agent and workflow use one Browser Workspace identity — all are fed by the worker's single `BrowserSession`.
- [x] There is no presentation-only duplicate target — terminal and Remote View consume the same session frame source.
- [x] Frame and semantic freshness is visible — target, browser revision, visual revision and frame age are reported separately.
- [x] Stale placement metadata is refused — geometry and remote-input tests require the current target/revision.

## Gate 5 — agent/browser integration

- [x] An attached browser dynamically enables observation tools — `BrowserAgentContext` adds `glass.browser.observe` only while authoritative context exists.
- [x] Mutation requires lease, policy and confirmation — the shared gateway and workspace mutation lease remain fail-closed.
- [x] Live semantic references resolve — attached context carries current target/browser revision and semantic summary.
- [x] Takeover/reconciliation is tested — workspace lease/takeover and Web IR continuity suites cover stale and ambiguous state.
- [x] Agent tools fail closed without dependencies — detached gateway capability tests omit browser/workflow tools.

## Gate 6 — product boundary

- [x] `glass-browser` remains independently useful and browser-focused — its default feature set excludes the development runtime.
- [x] `glass-dev` owns development orchestration — the full package enables the one-way `development-runtime` feature.
- [x] There is no cyclic dependency — only `glass-dev -> glass-browser` exists.
- [x] Browser-only installs avoid the full development runtime — no-default-feature check and package inspection enforce this.
- [x] Both crates package/install independently — the isolated clean-install smoke installs each extracted crate.

## Gate 7 — development reliability

- [x] Process-tree lifecycle is proven — Unix process-group and Windows Job Object ownership are implemented; the Unix descendant test verifies cleanup.
- [x] Project-tree truncation is explicit — `ProjectTreeResult` contains `truncated`, `limit`, ignored directories and skipped symlinks.
- [x] Ignore semantics protect the scan budget — generated directories are excluded before traversal and covered by tests.
- [x] Editor saves remain conflict-safe — atomic save and external-change/claim conflict tests remain green.
- [x] Language services persist and shut down correctly — one resident TUI client tracks didOpen/change/save/close and sends shutdown/exit before reap.

## Gate 8 — cross-platform and release truth

- [x] Browser-free CI covers Linux, macOS and Windows — the CI matrix checks and tests the workspace on all three.
- [x] Support claims are evidence-backed — docs separate source/CI coverage from Linux native-browser evidence.
- [x] Clean-install smoke covers both crates — `scripts/smoke-clean-install.sh` verifies core and full products in isolated roots.
- [x] Release history language is correct — published 0.3.2 evidence remains historical and 0.3.3 is explicitly local-only.
- [x] Major limitations are documented — remote graphics, Mosh, loopback forwarding, platform and native-browser claim boundaries are explicit.

## Gate 9 — full-suite install completeness

- [x] `cargo install glass-dev` exposes `glass` — the package declares and install-smokes the binary.
- [x] `cargo install glass-dev` exposes `glass-browser` — its thin launcher delegates to the shared browser CLI dispatch.
- [x] A full-suite user needs no second crate install — both executables come from one package.
- [x] Core-only `cargo install glass-browser` remains supported — it is packaged and installed separately.
- [x] Core-to-full and full-to-core transitions are tested/documented — isolated force replacement plus explicit uninstall/install guidance cover both directions.
- [x] Versions and help are coherent — synchronized 0.3.3 metadata and installed `--version`/`--help` checks cover all binaries.

## Gate 10 — zero-exit browser recovery

- [x] An occupied preferred port does not force TUI exit — the long-lived worker enters Recovery while the App/project runtime remains alive.
- [x] An unrelated listener offers automatic free-port launch — classification is fail-closed and `browser launch --port auto` remains available.
- [x] A compatible Chrome endpoint attaches from the TUI — fresh bounded CDP proof is required immediately before attach.
- [x] Multiple page targets are selectable in the TUI — `browser targets [PORT]` and numbered/ID `browser attach` use privacy-projected metadata.
- [x] Disconnect, reconnect, launch and attach work after startup — the live controller accepts each operation without rebuilding the TUI.
- [x] Project, agent and process runtime survives recovery — only `BrowserSession` is replaced; the resident `App` and `ProjectWorkspace` are retained.
- [x] Semantic truth is invalidated/refreshed across reconnection — Connecting clears target/revisions/content; Ready immediately queues fresh observation.
- [x] Phone layout exposes the same recovery actions — recovery is a stacked attention card and every action has a printable palette command.

## Release boundary

The issue implementation is complete only as a local candidate. Closing the
remote issue, pushing commits, creating `v0.3.3`, publishing crates, and
creating a GitHub Release require explicit maintainer approval.
