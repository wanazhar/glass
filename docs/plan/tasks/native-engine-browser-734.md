id: native-engine-browser-734
scope: glass-browser/persistent-process-modal-dialog-continuation-and-target-close
status: complete
depends-on: [native-engine-browser-733]
---

# Glass native-engine browser slice 734: modal dialog continuation

## Objective

Replace the process-backed native page's placeholder `alert`/`confirm`/`prompt`
returns with a real modal decision round trip in the persistent native owner.
Only a session with an active out-of-band owner control loop may enable the
blocking host call. While a page script is stopped inside a dialog call, the
owner must keep servicing bounded status and dialog-resolution controls,
without allowing a second browser-state writer. It also needs an exact
dialog-identity target-close control for a suspended process-backed HTTP(S)
navigation.
The original script resumes once at the call site with the selected result;
it must not be re-evaluated or report placeholder `false`/`null` as if a user
had answered.

This is a foundational integration slice, not completion of issue #40. Its
scope is process-backed HTTP(S) page execution—including an explicit
`evaluate()` after commit—and the persistent native-owner control path. A
standalone session without an independent control loop keeps the existing
event-queue path until its CLI/MCP/TUI controller is connected; it must never
block in an unserviceable modal call. Direct in-process realms, public
standalone-session resolution, user-facing TUI prompt presentation,
`beforeunload`, and native-only cross-platform certification remain separate
gates. The direct content-process evaluate probe initially appeared to return a
queued projection, but it was running a stale `glass-native-content-worker`
binary; rebuilding that worker and rerunning the test verified modal
continuation for explicit evaluation.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-733.md`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/persistent.rs`

## Contract

- In a modal-control-enabled persistent owner, a process-backed page script
  pauses at `alert`, `confirm`, or `prompt` until a control request resolves
  that exact pending dialog, whether the call is in a page-load script or an
  explicit `evaluate()` operation. The child content process remains the sole
  JavaScript execution owner. An owner without a live control loop must not
  install the blocking host callback.
- `alert` returns `undefined` after acceptance. Its no-argument overload uses
  the empty message, while explicit `undefined` and `null` undergo string
  conversion (`"undefined"` and `"null"`). `confirm` returns `true` on
  acceptance and `false` on dismissal. `prompt` returns the accepted bounded
  response string (defaulting to the provided default value when no explicit
  response text is sent) or `null` on dismissal. Other arguments follow their
  Web IDL string conversion and declared optional defaults.
- The suspended command is not completed, restarted, or re-evaluated when the
  dialog opens. Resolution resumes it once; mutations after the dialog become
  visible only after that decision.
- While the command is suspended, the persistent owner continues polling it
  and handles status plus exact-dialog-identity accept/dismiss controls. A
  revision-checked `closeDialogTarget` control may cancel only the active
  process-backed HTTP(S) navigation that owns the exact pending dialog; the
  owner closes that target after the cancelled operation releases its lock.
  Each dialog identity is bound to its owning context and frame; other
  concurrent browser operations are rejected as busy.
- Dialog data and prompt response text have explicit byte bounds. Invalid,
  stale, duplicate, or cross-target resolutions return typed errors and do not
  resume the script.
- The content-operation deadline pauses while a modal awaits a human decision
  and resumes after resolution. Navigation cancellation, exact target close,
  owner shutdown, and content-worker exit release the rendezvous and cannot
  strand the owner or allow a late answer to mutate a replacement document.
  This slice does not impose a deadline on the human's decision.
- No CDP, remote browser, or implicit fallback is used.

## Integration paths

1. `window.alert/confirm/prompt` in the content-process QuickJS realm emits a
   bounded dialog-open event and synchronously waits for its matching answer.
2. `NativeContentProcess` multiplexes that event while its original request is
   pending, and forwards a matching decision to the same child process.
3. The native runtime/owner exposes the live pending prompt and a separate
   control-plane resolution path that does not acquire the busy page-state
   lock.
4. Persistent-owner request handling accepts dialog controls alongside
   status, while preserving one active browser-state writer. An exact
   `closeDialogTarget` cancels an active HTTP(S) navigation and defers closing
   its owning target until that operation has released the state lock.
5. The browser command returns only after the content process resumes the
   script and completes the original operation, or the selected cancellation
   path has terminated it.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/browser/persistent.rs`
- `crates/glass-browser/src/browser/session/types.rs`
- `crates/glass-browser/src/browser_backend.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/tasks/native-engine-browser-734.md`

## Verification

- Add deterministic process-backed page-load fixtures that exercise accepted
  and dismissed `alert`, `confirm`, and `prompt` calls and assert exact script
  continuation order/results, prompt default/explicit response, and no
  execution after the call before resolution. Also resolve a confirm, prompt,
  and alert from one explicit `evaluate()` operation after the document commits.
- During a suspended dialog, prove the same persistent owner returns status,
  rejects a second state-changing command as busy, rejects stale/wrong-target
  resolution and stale target-close revisions, and resumes the original
  request after a valid decision.
- Hold a process-backed dialog beyond several short content-operation
  deadlines, then resolve it and prove the original script completes. Also
  prove navigation cancellation, exact target close, owner shutdown, and
  content-worker exit release the operation; a fresh worker and parked sibling
  target remain usable afterward.
- Run one locked package-scoped `glass-browser` check before focused
  process/owner tests; then run formatting, whitespace, release-truth,
  documentation-depth, and shortcut gates after the coherent batch.
- Keep direct in-process/TUI/cross-platform gaps explicitly open; do not claim
  issue #40, browser completeness, remote CI, or production certification.
- Do not run workspace-wide tests, `cargo clean`, or build `glass-dev` merely
  for the documentation-coverage verifier.
