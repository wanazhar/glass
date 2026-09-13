# Native persistent first-class surfaces (322)

Status: implemented locally in the native runtime.

This slice completes the persistent-session attachment seam for the two
remaining first-class browser surfaces. `glass browser --session NAME` and
`glass tui --session NAME` now attach a non-owning TUI proxy to a running native
owner. Navigation, semantic observation, target listing/selection, actions,
screenshots, and history/loading controls all reach the owner; the TUI never
creates a second engine and closing the TUI never stops the named owner.

The native MCP entrypoint accepts `--session NAME` and multiplexes browser tool
parameters through the same private mode-0600 Unix socket. The owner invokes
the existing native MCP dispatcher against its in-memory session, so browser
state, profile/storage policy, viewport, revisions, and policy authority stay
in one process. Browser-free Web IR, task validation/compilation, knowledge,
experience, and snapshot inspection operations remain local to the MCP client
when they do not require the live page.

The owner protocol adds bounded revision-checked controls for back, forward,
reload, and stop-loading. Persistent clients reject stale revisions and
unsupported controls explicitly. The existing Chromium record format remains
backward-compatible, and native liveness continues to use the owner PID rather
than a fabricated CDP port or browser child PID.

Focused validation covers the native owner’s command, control, and MCP paths,
the TUI attachment projection, both native and no-default feature checks, and
the existing native runtime/MCP tests. The remaining issue #40 work is native
workflow/batch resume, richer semantic regions and Web IR projection, and the
full Core Web Profile platform/security/recovery/performance certification.
