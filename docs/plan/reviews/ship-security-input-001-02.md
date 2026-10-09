# Ship security and MCP integration review 001-02

Review date: 2026-10-09

Reviewed the frozen integration commit `fbc56a03335e1f32e1b6f0096524326313921c4d` and the root's uncommitted MCP input hardening in `crates/glass/src/input.rs`. This review covers permission approvals and profiles, extension/MCP input handling, provider diagnostic redaction, and doctor output. It does not approve the whole 20-item release.

## Findings and disposition

- The original MCP elicitation parser accepted hidden control characters and oversized `oneOf`/array enum constants and defaults. The UI could display a safe title while submitting a hidden constant. The root added a shared value bound/control check across the four enum forms, defaults, and final responses, plus a property-name check. The independent focused input suite passes 6/6 on the patched working tree, including `hidden_enum_constants_and_defaults_are_rejected`.
- Exact command exceptions are held in process memory, cleared on REPL start, `/clear`, session switch, and `/branch`, and copied into each interactive Agent worker before dispatch. Headless/delegated agents have no key reader and receive empty approval snapshots; extension host Bash callbacks also use an empty approval set. Background jobs use only an exact stored command and the workspace cwd. The PTY probe exercised exact foreground and `/bg` approvals, denial after `/clear`, and saving a profile for the next launch.
- Profile reporting distinguishes the configured next-start profile from the effective startup profile. Persistence parses the existing TOML, changes only the root key, verifies the parsed result, and atomically saves it. The PTY probe confirmed the current process keeps its startup policy after saving `read-only`.
- MCP elicitation is wired from the interactive Agent through the real bridge and shared KeyReader; the callback result is not stored by Glass. Headless calls have no input handler and receive the explicit `interactive_input_unavailable` response. Form, URL, refusal, bounds, and fresh-ID/state/argument mapping paths are covered by parser/protocol tests; the real PTY probe confirms a submitted sentinel is absent from session data.
- Provider errors use one redacting `Display` path, Agent warnings are sanitized before durable system records, stream errors are sanitized before live display and error records, and serve retry/fallback/error progress uses the same sanitizer. Doctor diagnostics use value-free labels and report configured/effective policy honestly. The error PTY probe confirms a bearer sentinel and OSC52 sequence are absent from output and the session log; doctor's offline and modal redaction probe passes.

## Independent verification

The supplied release binary for `fbc56a0` had SHA-256 `80b42e4974b94dd0588f7d5a8d967f10d3f645db3f4ddd3f27cedd297910e16d`. Against it:

- `scripts/smoke-permissions-pty.py` passed at 40x24 and 20x8.
- `scripts/smoke-elicitation-pty.py` passed at 40x24 and 20x8.
- `scripts/smoke-mcp.py`, `scripts/smoke-chat.py`, and `scripts/smoke-doctor.py` passed.

On the patched working tree, `CARGO_NET_OFFLINE=true cargo test --locked -p glass-core input::tests:: -- --test-threads=1` passed 6/6. The updated `scripts/smoke-workflows.py` still fails its 20x8 assertion because it waits for the full modal title `Review extension capabilities`; the actual clipped title is `Review extension capabil`. The test needs a compact-size assertion. This is a test assertion issue; the workflow had already passed through the prompt sequence at larger sizes.

The supplied release binary predates the uncommitted MCP parser hardening and workflow assertion update. Final release verification must rebuild the exact final root tree and rerun the affected focused probes. No remaining source blocker was found in this review after the parser patch; the tiny-terminal workflow assertion remains outstanding.
