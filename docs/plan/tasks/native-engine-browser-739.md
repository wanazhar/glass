id: native-engine-browser-739
scope: glass-browser/native-mcp-modal-dialog-host
status: in-progress
depends-on: [native-engine-browser-738]

# Glass native-engine browser slice 739: MCP modal-dialog host

## Objective

Allow normal one-process native MCP browser calls to complete process-backed
JavaScript `alert`, `confirm`, and `prompt` dialogs through the connected MCP
client's human elicitation UI, without blocking the stdio reader or falling
back to Chromium/CDP.

## Context

- `docs/INDEX.md`
- `docs/mcp.md`
- `docs/architecture/native-engine.md` — MCP dialog host contract
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-738.md`
- MCP `2025-11-25` elicitation specification:
  <https://modelcontextprotocol.io/specification/2025-11-25/client/elicitation>

## Contract

- Continue accepting MCP `2024-11-05` clients. Also accept
  `2025-11-25`; preserve the client's selected version in the initialize
  result. Reject unsupported versions explicitly.
- Enable modal native sessions only when `2025-11-25` was negotiated and the
  client declares form-mode `elicitation`. Other clients retain the existing
  non-modal path and cannot be left waiting for human input.
- Send `elicitation/create` only while processing an active browser tool call.
  The stdio reader routes the server-request response independently while the
  original browser operation remains alive and owns the session lock.
- Map alert to acknowledgement, confirm to a boolean decision, and prompt to
  response text. Validate response action and structure; enforce a 256-byte
  UTF-8 prompt-response limit even when the client's JSON Schema validation is
  character-based.
- Resolve the exact pending dialog identity. Decline/cancel dismisses confirm
  and prompt (`false`/`null`); an alert cannot return a value. Never include the
  source URL or prompt response in logs. Escape control/bidirectional-format
  text, omit URL-like page text, mark page content untrusted, and warn users not
  to enter credentials or other secrets in response to a page prompt.
- Pause the MCP navigation deadline while a human dialog is open. Request
  cancellation and stdio EOF must cancel nested elicitation, dismiss the owned
  modal through the live IPC exchange, and release the suspended page
  operation. If it cannot drain promptly, discard the owned native session.
- This slice covers standalone `glass --mcp` native sessions. Persistent MCP
  sessions, MCP 2026-07-28 multi-round-trip negotiation, in-process dialogs,
  `beforeunload`, cross-platform certification, and other Core Web Profile
  gates remain open issue #40 work.

## Tradeoffs

The existing 2024 protocol remains usable without modal UI, while interactive
2025 clients gain a standards-defined host request. This avoids silently
stranding older automation clients. The server adds one pending-response map
and a small controller poll while a modal is active; no idle poll runs for
ordinary browser calls. The Elicitation response is validated again by Glass,
because client-side schema validation is not trusted.

## Path

- `crates/glass-browser/src/mcp/server.rs`
- `docs/mcp.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-739.md`

## Verification

- Unit-test version negotiation, client capability parsing, server request
  serialization, response correlation, action mapping, control-text escaping,
  and UTF-8 byte bounds.
- Exercise a real process-backed local-HTTP page through the stdio MCP
  transport: prompt → confirm → alert, including delayed human replies beyond
  the navigation deadline, parent-request cancellation, and stdio EOF while a
  dialog is open. Prove no Chromium/CDP process/socket is used.
- Run one scoped `glass-browser` check before affected tests, then formatting,
  documentation inventory/truth gates, and `git diff --check`. Keep Cargo
  validation batched under the repository Rust execution policy.
- Remote CI and Windows/macOS native certification must be separately recorded;
  do not claim them from local Linux evidence.

## Local evidence

- `cargo check -p glass-browser --lib --tests --locked --quiet` — passed.
- `cargo test -p glass-browser --lib native_mcp_stdio_elicitation_resumes_process_page_dialogs --locked -- --nocapture` — passed. Covers the real process-backed dialog sequence, navigation deadline pause, parent cancellation followed by a same-session `getText`, and stdio EOF.
- `cargo fmt --all -- --check` and `git diff --check` — passed.
- Documentation truth, depth/index, TUI shortcut, and coverage checks — passed; 1367 Markdown files inventoried with 0 current-claim failures.
- Remote CI and Windows/macOS native certification remain pending; keep this task `in-progress` until those gates are recorded.
