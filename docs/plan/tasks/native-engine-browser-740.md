id: native-engine-browser-740
scope: glass-browser/persistent-native-mcp-dialog-host
status: done
depends-on: [native-engine-browser-739]

# Glass native-engine browser slice 740: persistent MCP dialog host

## Objective

Complete native page-dialog handling for MCP tools attached to a named
persistent native session. The MCP process owns elicitation; the persistent
owner remains the sole browser-state writer and exposes pending-dialog status
and exact-ID decisions through its existing out-of-band control socket.

## Context

- `docs/INDEX.md`
- `docs/mcp.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-739.md`
- `crates/glass-browser/src/browser/persistent.rs`
- `crates/glass-browser/src/browser/native_engine/dialog.rs`
- `crates/glass-browser/src/browser/session/types.rs`
- `crates/glass-browser/src/mcp/server.rs`

## Contract

- For `2025-11-25` form-elicitation clients, route persistent-session page
  dialogs through the same sanitized alert/confirm/prompt schema as standalone
  native MCP. Preserve the persistent owner's browser session and resolve only
  the pending dialog ID returned by that owner.
- Poll the owner only while a persistent MCP browser tool is active. Return the
  pending dialog and active navigation revision in the owner status response;
  do not add a third crate, browser process, or alternate state writer.
- Pause the MCP navigation deadline while a human response is pending. On
  parent cancellation or stdio EOF, cancel the nested elicitation, dismiss the
  exact pending dialog, stop an interruptible active navigation, and drain the
  original MCP operation before returning. Keep the persistent owner usable.
  The persistent owner must not race this with a second, non-pausable MCP
  navigation timer; the outer dialog host owns that deadline for this request.
- If the client did not negotiate form elicitation, do not wait for human
  input: dismiss the exact dialog, stop active navigation when possible, drain
  the operation, and return an explicit MCP tool error.
- On a stale dialog ID, never apply a response to a newer dialog. Re-query
  status within a bounded retry; then return a typed failure without silently
  continuing with the wrong response.
- Continue to reject unsupported MCP versions and keep the 2026-07-28
  multi-round-trip protocol, `beforeunload`, and non-Unix persistent session
  support outside this slice.
- Never start Chromium/CDP or silently fall back.

## Tradeoffs

The MCP host polls the existing Unix owner-control socket while one browser
operation is active. A 100 ms interval bounds human-dialog presentation delay
without creating an idle watcher. The owner retains browser-state ownership;
the caller only observes status and sends exact-ID controls. This slice relies
on the existing Unix-only persistent-session transport and does not change the
standalone native MCP platform contract.

## Path

- `crates/glass-browser/src/browser/persistent.rs`
- `crates/glass-browser/src/browser/native_engine/dialog.rs`
- `crates/glass-browser/src/browser/session/types.rs`
- `crates/glass-browser/src/mcp/server.rs`
- `docs/mcp.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-740.md`

## Verification

- Test the owner status shape with no dialog and with an exact pending dialog;
  verify active navigation revision reporting and exact-ID resolution.
- Run a process-backed local-HTTP end-to-end flow through `glass --session
  NAME --mcp`: prompt, confirm, alert, delayed response beyond the navigation
  deadline, cancellation, and stdio EOF. Verify the same persistent browser
  session remains usable after cancellation.
- Verify clients without form elicitation receive an explicit error and the
  persistent owner is not left blocked. Prove no Chromium/CDP process/socket is
  used.
- Run one scoped `glass-browser` check before affected tests, formatting,
  documentation inventory/truth gates, and `git diff --check`.
- Record remote CI separately. Persistent-session control is Unix-only; do not
  claim Windows/macOS persistent-session support from Linux evidence.

## Local evidence

- `cargo fmt --all -- --check` — passed.
- `cargo check -p glass-browser --lib --tests --locked --quiet` — passed.
- `cargo test -p glass-browser --lib native_persistent_mcp_stdio_elicitation_keeps_owner_usable --locked --quiet -- --nocapture` — passed. Exercises prompt/confirm/alert through real stdio MCP and local HTTP, a human response delayed beyond the navigation deadline, legacy-client dismissal, parent cancellation, and stdio EOF; after each interruption the same owner answers another native MCP call with no pending dialog.
- The owner record remained `BrowserRuntime::Native`, `browser_pid == 0`, and `port == 0`; no Chromium/CDP session was started.
- `python3 scripts/check-release-documentation.py --require-previous-version` — passed: 1,368 Markdown documents, zero current-claim failures.
- `python3 scripts/check-documentation-depth.py` — passed: 93 current guides and 19 substantive contracts.
- `python3 scripts/check-tui-shortcuts.py` — passed: 15 implementation help keys and 63 documentation markers.
- `python3 scripts/check-documentation-coverage.py` — passed: 1,368 Markdown files, 346 full-product MCP tools (101 browser-only), 17 examples, and 22 public modules.
- `git diff --check` — passed.
- Remote CI and Windows/macOS persistent-session certification were not run; they remain issue-level gates and are not claimed by this Linux slice.
