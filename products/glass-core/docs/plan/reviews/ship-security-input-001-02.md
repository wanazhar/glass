# Ship security and MCP integration review 001-02

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Review date: 2026-10-09

Reviewed final tree `1890e599b953864401f79ef9ca94291956545382`, including MCP input hardening and the review-fix commit `9c895e39507335ef46af68f78990550db947ceec`. This review covers permission approvals and profiles, extension/MCP input handling, provider diagnostic redaction, and doctor output. It does not approve the whole 20-item release.

## Findings and disposition

- The original MCP elicitation parser accepted hidden control characters and oversized `oneOf`/array enum constants and defaults. The UI could display a safe title while submitting a hidden constant. The root added a shared value bound/control check across the four enum forms, defaults, and final responses, plus a property-name check. The independent focused input suite passes 6/6 on the patched working tree, including `hidden_enum_constants_and_defaults_are_rejected`.
- Exact command exceptions are held in process memory, cleared on REPL start, `/clear`, session switch, and `/branch`, and copied into each interactive Agent worker before dispatch. Headless/delegated agents have no key reader and receive empty approval snapshots; extension host Bash callbacks also use an empty approval set. Background jobs use only an exact stored command and the workspace cwd. The PTY probe exercised exact foreground and `/bg` approvals, denial after `/clear`, and saving a profile for the next launch.
- Profile reporting distinguishes the configured next-start profile from the effective startup profile. Persistence parses the existing TOML, changes only the root key, verifies the parsed result, and atomically saves it. The PTY probe confirmed the current process keeps its startup policy after saving `read-only`.
- MCP elicitation is wired from the interactive Agent through the real bridge and shared KeyReader; the callback result is not stored by Glass. Headless calls have no input handler and receive the explicit `interactive_input_unavailable` response. Form, URL, refusal, bounds, and fresh-ID/state/argument mapping paths are covered by parser/protocol tests; the real PTY probe confirms a submitted sentinel is absent from session data.
- Provider errors use one redacting `Display` path, Agent warnings are sanitized before durable system records, stream errors are sanitized before live display and error records, and serve retry/fallback/error progress uses the same sanitizer. Doctor diagnostics use value-free labels and report configured and effective policy. The error PTY probe confirms a bearer sentinel and OSC52 sequence are absent from output and the session log; doctor's offline and modal redaction probe passes.

## Independent verification

The final release binary for `1890e59` has SHA-256 `8d81bae65e14f8aeb6cf131f41cd299dbde9dbc3ca49906fa82ee03132d3aa93`. Against this exact binary:

- `scripts/smoke-permissions-pty.py` passed at 40x24 and 20x8.
- `scripts/smoke-elicitation-pty.py` passed at 40x24 and 20x8.
- `scripts/smoke-mcp.py`, `scripts/smoke-chat.py`, and `scripts/smoke-doctor.py` passed.
- `scripts/smoke-workflows.py` passed all six workflows at 80x24, 40x12, 24x10, and 20x8.

On the patched working tree, `CARGO_NET_OFFLINE=true cargo test --locked -p glass-core input::tests:: -- --test-threads=1` passed 6/6. The workflow smoke assertion was then shortened for compact terminals; `/tmp/gc-review-fix-workflows.log` records `scripts/smoke-workflows.py` passing all six workflows at 80x24, 40x12, 24x10, and 20x8, including extension capability review and enablement.

Root reports that the final workspace test suite (571 tests) and strict Clippy pass; I have not independently rerun those checks. The focused PTY probes above were independently rerun on the exact final release. No remaining source blocker was found in this review.
