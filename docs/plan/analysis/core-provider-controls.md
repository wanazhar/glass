# Core-owned provider authentication and model controls

## Current behavior

Glass Core is the default resident harness; Pi is selected only with
`GLASS_AGENT_HARNESS=pi`. The Agent command palette already has separate Core
and Pi catalogs, but the Core catalog calls its login action “guidance” and
its model/session actions still use labels inherited from the Pi surface.
Readiness errors and command output also contain Pi-specific setup wording.

Core owns provider configuration, credentials, selected model, and the
interactive sign-in flow. Its ordinary terminal UI implements `/login` and
`/model`; the resident JSON-RPC protocol exposes `auth/status`, `models/list`,
and `session/configure`, but intentionally has no interactive login RPC.
Glass's current Core login action only prints instructions. It must not copy
provider credentials into Glass or implement a second credential store.

The existing active-agent model request path already crosses the negotiated
Core runtime adapter: `PiSessionRequest::Models` maps to `models/list`, and
`SetModel` maps to `session/configure`. Glass will keep that transport for the
currently selected resident session, but present it as Core-owned control,
preserve Core's response/model catalog, and test the exact protocol mapping.
No model operation will write provider configuration in Glass.

## Ownership and interaction

| Control | Glass Core default | Explicit Pi legacy mode |
|---|---|---|
| Login | Suspend Glass terminal UI, launch the resolved Core executable in its own interactive mode with inherited stdio, resume Glass, refresh readiness | Preserve managed Pi login flow and Pi-specific labels |
| Readiness/setup | Describe Core runtime and Core-owned provider setup | Describe Node, managed Pi SDK, and Pi provider readiness |
| Model list/selection | Send `models/list` / `session/configure` through the active negotiated Core session | Preserve the Pi adapter's existing model request behavior |
| Credentials | Remain in Core's existing account/configuration storage | Remain in the legacy Pi runtime's store |

The login child uses the existing binary resolution order: explicit
`GLASS_CORE_BINARY`, then `glass-core` and `gc` on `PATH`. Glass passes no
provider token, key, login hint, or credential path. Core's interactive UI
starts in the workspace directory and owns its own configuration lifecycle.
After it exits, Glass resumes its terminal and reports the new Core readiness
state. The already-running resident runtime retains its in-memory per-session
configuration; even `:agent new` opens a session in that same process. Glass
will not restart it, including when a turn may be active. The handoff result
explains that the user must start a new independent resident agent/runtime
process (for example, `:agent spawn`) to use newly selected Core settings;
the existing conversation remains available. This avoids an unrequested
process restart and preserves the old session reference.
If the binary cannot be resolved or the terminal cannot be suspended, Glass
gives an actionable error without pretending login occurred.

```text
Glass TUI (:agent setup login)
    │ suspend its terminal guard
    ▼
resolved glass-core executable (inherited stdin/stdout/stderr, workspace cwd)
    │ human uses Core's /login, then exits Core
    ▼
Glass resumes terminal guard and refreshes Core auth/status
    │ existing runtime/session remains untouched; a new independent agent process reads fresh config
```

The command catalog remains selected by the configured harness. Core wording
must not call its controls Pi setup or Pi login; Pi labels remain truthful when
the user explicitly selects the legacy harness. Readiness remediation follows
the same split.

## Acceptance and verification

- [x] Core `:agent setup login` performs an actual terminal handoff to Core's
      own interactive `/login`; Glass neither prompts for nor stores secrets.
- [x] The child is resolved from the configured Core binary/PATH, receives
      inherited terminal streams and workspace cwd, and the TUI resumes and
      refreshes readiness after exit or reports launch/resume failure. It does
      not restart the current runtime; the result explains that a new
      independent agent process (not `:agent new` in the old process) is needed
      to use updated Core settings while preserving the old session.
- [x] Core action labels and readiness text name Glass Core and its provider
      flow; explicit Pi mode retains its Pi-specific setup/login labels.
- [x] Core model-list and model-selection commands target the selected
      resident session and map to negotiated `models/list` and
      `session/configure`, including provider/model selection; they do not
      route credentials or config through Glass.
- [x] Fixture tests cover model request mapping and errors; Core and Pi
      command/catalog behavior is tested without live provider credentials.
- [x] A fake executable plus PTY or process fixture verifies terminal
      handoff arguments, cwd, inherited streams where feasible, and return to
      Glass without network or auth use.
- [x] Current runtime/harness/CLI docs explain ownership, handoff, selected
      harness behavior, and model controls; historical records remain intact.
- [x] Focused `glass-dev` tests, formatting, strict scoped Clippy, docs
      validators, and final diff review pass. No push, release, global install,
      service restart, or issue closure.
