id: native-engine-browser-741
scope: glass-browser/native-beforeunload-confirmation
status: done
depends-on: [native-engine-browser-740]

# Glass native-engine browser slice 741: before-unload confirmation

## Objective

Implement the native navigation decision point for a canceled `beforeunload`
event, including sticky-activation eligibility and a user-visible exact-ID
confirmation through the existing CLI, TUI, and MCP dialog hosts. Keep the
original document and navigation operation owned by the native engine; do not
start CDP or add another state writer.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-046.md`
- `docs/plan/tasks/native-engine-browser-740.md`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/dialog.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/session/types.rs`
- `crates/glass-browser/src/cli/runner.rs`
- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/render.rs`
- `crates/glass-browser/src/mcp/server.rs`
- HTML Standard: [unloading documents]

## Contract

- Dispatch the outgoing active Document's `beforeunload` before `pagehide`,
  `unload`, replacement-resource requests, or history commit, including
  cross-document asynchronous Back/Forward traversal through `BrowserSession`.
- A non-canceled event proceeds normally. A canceled event proceeds without a
  prompt when the Document has no sticky activation. Glass-delivered browser
  input (click, keyboard/text, selection, upload, and drag actions) grants
  sticky activation; script `dispatchEvent()` and `HTMLElement.click()` do
  not. A successful document replacement resets activation for the new
  Document; same-document changes and dismissed prompts preserve it.
- When cancellation and sticky activation require a prompt, expose one pending
  dialog of type `beforeunload`, with no author-provided text or prompt value.
  The terminal, TUI, and MCP surfaces use generic user-agent copy and never
  display the page's `returnValue` string.
- Accept continues the exact suspended navigation once. Dismiss cancels it,
  leaves the outgoing document active, and does not dispatch `pagehide` or
  `unload`, fetch the replacement resource, or commit its history entry.
  Callback mutations made by `beforeunload` remain on the current document.
- A dismissed cross-document Back/Forward confirmation leaves the current
  history entry selected and does not request the target URL. Accept requests
  and activates that exact history target once; an already loaded history
  document may be restored without another network request. Same-document
  history traversal does not dispatch `beforeunload`.
- An eligible prompt without an enabled responsive controller returns an
  explicit error and preserves the outgoing document; it neither silently
  accepts nor leaves an unobservable operation suspended.
- CLI EOF/cancellation, TUI dismissal/quit, MCP cancellation, and stale IDs
  resolve or drain the exact dialog safely and leave the same native owner
  usable. Persistent MCP uses its existing status/control channel.
- No more than one browser-level `beforeunload` prompt is shown for one
  navigation attempt. This slice handles the outgoing active document owned by
  the current engine. Traversing descendant-frame unload handlers and
  propagating sandboxed-modals flags remain explicit follow-up profile work;
  do not claim those complete.
- No Chromium/CDP launch, socket, fallback, or implicit acceptance.

## Tradeoffs

Sticky activation is per outgoing document and begins only after a Glass input
action that represents trusted browser input. The first release of this slice
does not claim general transient-activation APIs, activation consumption, or
descendant-frame propagation. Without a responsive host, the operation fails
closed with a typed error rather than silently dropping the prompt.

## Path

- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/dialog.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/cli/runner.rs`
- `crates/glass-dev/src/browser.rs`
- `crates/glass-dev/src/tui/state.rs`
- `crates/glass-dev/src/tui/render.rs`
- `crates/glass-browser/src/mcp/server.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs` tests
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-741.md`

## Verification

- Check both process-backed HTTP(S) and local-document navigation paths.
- Check process-backed HTTP(S) Back/Forward so the target request cannot begin
  before the exact `beforeunload` decision.
- Prove no-activation cancellation proceeds without a prompt; trusted input
  followed by cancellation opens exactly one `beforeunload` prompt; script
  generated clicks do not create activation.
- Prove accept issues exactly one replacement request and orders
  `beforeunload`, `pagehide`, `unload`, then new-document publication. Prove
  dismiss leaves URL/document/history unchanged, keeps `beforeunload` callback
  mutations, emits no `pagehide`/`unload`, and issues no replacement request.
- Verify the CLI decision parser, TUI rendering, MCP elicitation copy/schema,
  and process-backed exact-ID accept/dismiss through the native session
  controller. The persistent owner reuses the same dialog-control contract;
  its existing owner-level cancellation and reuse tests remain in the earlier
  dialog-host slices.
- Run one affected-package `cargo check` before focused tests, followed by
  formatting, documentation truth/inventory gates, and `git diff --check`.
- Record remote CI and cross-platform certification separately; local Linux
  evidence must not be presented as multi-platform promotion.

## Local evidence

2026-09-25 Linux checks:

- `cargo check -p glass-browser --lib --tests --locked --quiet` passed.
- `cargo check -p glass-dev --lib --bins --locked --quiet` passed.
- `cargo test -p glass-browser --lib beforeunload --locked --quiet` passed
  five tests, including local cross-document history confirmation.
- `cargo test -p glass-browser --test native_engine beforeunload --locked --quiet`
  passed four host/navigation/history tests.
- `cargo test -p glass-browser --test native_engine native_content_process_history_beforeunload_waits_before_loading_target --locked --quiet` passed the process-backed Back/Forward request-order and accept/dismiss test.
- `cargo test -p glass-browser --lib mcp_elicitation_resolves_dialog_semantics_and_bounds_prompt_bytes --locked --quiet` passed.
- `cargo test -p glass-browser --lib native_cli_dialog_responses_preserve_web_dialog_semantics --locked --quiet` passed.
- `cargo test -p glass-dev --lib beforeunload --locked --quiet` passed two
  tests for overlay copy and decision status.

These checks are local Linux evidence only. They do not certify Windows,
macOS, remote CI, descendant-frame traversal, or sandbox-modal propagation.
At this slice's completion, direct synchronous `NativeEngine` history helpers
still lacked cross-document lifecycle handling; slice 742 supersedes that
specific gap. Windows/macOS, remote CI, descendant-frame traversal, and
sandbox-modal propagation remain issue #40 gates.

[unloading documents]: https://html.spec.whatwg.org/multipage/browsing-the-web.html#unloading-documents
