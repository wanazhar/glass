# Glass v0.3.6 issue #36 gate review

Status: local candidate certification in progress on 2026-08-12

This review maps issue #36 scenarios A-J, release gates 1-15, and forbidden
outcomes to concrete source and executable evidence. Public/tag items remain
unchecked until their records exist.

## Integrated scenarios

- [x] A — first-run Agent readiness, explicit managed Pi setup/status, direct
      composer/session flow, browser context, and frozen one-use mutation
      confirmation are implemented and interaction tested.
- [x] B — one browser session performs navigation, semantic observation,
      revision-bound activation/type/scroll, workflow run/verify, target
      recovery, screenshot/live presentation, and Remote View operations.
- [x] C — Code opens, edits, navigates, saves, undoes, and redoes real native
      buffers while diagnostics and source/runtime context remain visible.
- [x] D — desktop Agent/Code/App/Terminal/Tasks/Git/Debug/More projections are
      reachable, locally scrollable, contextual, and mutation-confirmed.
- [x] E — 48x18, 64x24, and 80x24 phone tests reach Agent/Code/App/Tasks/More
      directly without cycling desktop internals.
- [x] F — normal browser selection/action derives the visible revision; stale
      failures invalidate semantic state and present recovery.
- [x] G — human takeover pauses Glass mutation ownership until explicit
      checkpoint reconciliation returns control.
- [x] H — daemon submit/inspect/events/cancel/reconcile retains stable bounded
      operation identity across disconnects without duplicating mutation.
- [x] I — terminal focus, mouse, paste, resize, raw-mode, alternate-screen,
      and cleanup behavior has unit and PTY coverage.
- [x] J — browser launch/attach/automatic-port/reconnect/target recovery is
      explicit; workspace state survives browser failure.

## Release gates

- [x] 1. Correctness debt: Agent workers join; panic is attributed; daemon work
      is recoverable; verification no longer defaults to settlement.
- [x] 2. Canonical BrowserWorkspace: one exported controller serves both TUIs.
- [x] 3. Live browser: explicit latest-frame Herdr and loopback Remote View.
- [x] 4. Semantic browser: bounded named entities, selection, highlight, stale
      invalidation, revision, and semantic-first rendering.
- [x] 5. Human browser actions: normal UI derives revisions automatically.
- [x] 6. Direct Agent chat: first destination, composer, conversation, setup,
      session controls, context, confirmation, and takeover rules.
- [x] 7. Real editor: files, buffers, cursor, edit, save, undo/redo, navigation,
      scrolling, and diagnostics are wired to actual services.
- [x] 8. Work-oriented desktop: seven primary destinations plus contextual More.
- [x] 9. Phone experience: five direct destinations and narrow-size tests.
- [x] 10. Pane interaction: focus, local scroll, mouse, paste, resize, and input.
- [x] 11. Designed projections: raw JSON is restricted to Inspect/detail state.
- [x] 12. Browser parity: shared operation inventory and adapter contract tests.
- [x] 13. Recovery: collision/attach/auto-port/reconnect/target/stale paths.
- [x] 14. Documentation: current architecture, controls, setup, migration,
      release notes, package boundary, limitations, and evidence are explicit.
- [ ] 15. Exact-tag certification: local full candidate, signed tag, native CI,
      fuzz, registries, GitHub Release, and issue closure must all pass.

## Forbidden outcomes

- [x] No second Browser implementation or duplicated action inventory.
- [x] No default screenshot/pixel automation context.
- [x] No hidden revision typing requirement for normal TUI actions.
- [x] No automatic global Pi install or secret logging.
- [x] No Agent settlement treated as verified success by default.
- [x] No daemon timeout used as mutation outcome.
- [x] No equal-weight subsystem wall or desktop shell shrunk into phone mode.
- [x] No unhandled enabled terminal input modes or raw JSON primary workspace.
- [x] No public release/platform claim without exact evidence.

## Certification record

Focused checkpoints passed canonical BrowserWorkspace (5), standalone TUI (3),
product TUI (8), Git (4), correctness, daemon, Pi, and strict warnings-denied
Clippy suites. Both package-owned TUIs passed PTY resize, focus, SGR mouse,
bracketed paste, ordinary input, bounded quit, alternate-screen, cursor, and
terminal-mode cleanup checks.

The clean candidate through documentation commit `df86f18` passed:

- locked all-target/all-feature tests: `glass-browser` 741 passed/1 ignored,
  `glass-dev` 127 passed, development integration 4 passed, plus all examples
  and integration suites;
- warnings-denied Clippy and rustdoc for both packages;
- all 18 live Chromium scenarios plus the resident BrowserService revision,
  Remote View, reload, process, and port-cleanup scenario;
- both packaged TUI PTY smokes, version/release/docs/coverage/depth/parity,
  reliability, adapter, and Web IR validators;
- verified `glass-browser` packaging; patched pre-publication `glass-dev`
  packaging with an exact normalized registry dependency; both publish dry
  runs aborted before upload;
- isolated fuzz all-target compilation, `cargo deny`, and `cargo audit` with
  only the allowed transitive `lru` advisory warning;
- optimized release and release-size builds reporting 0.3.6;
- isolated core/full installs, core-to-full, full-to-full, full-to-core, and
  published 0.3.5-to-candidate 0.3.6 upgrade transitions.

The public exact-tag records will be appended before Gate 15 is checked.
