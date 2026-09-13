# Native engine persistent session ownership (321)

Status: implemented locally in the native runtime.

This slice makes the named persistent-session owner runtime-aware. A native
session can start one Glass-owned `BrowserRuntimeSession`, publish a durable
runtime/profile/status record, and keep that in-memory browser state alive
between CLI invocations. The owner is reachable only through its mode-0600
Unix socket and remains the only process that can close the native session.

Native `glass --session NAME <command>` requests send the original command
arguments to the owner. The owner reparses and validates them, enforces its
fixed profile/storage/viewport contract, reuses the existing native command
dispatcher, and returns the same typed JSON or text output envelope. This
keeps command behavior in one dispatcher instead of creating a second
persistent-command implementation. Request bytes are bounded, session names
are validated, and mismatched profiles or per-call storage/viewport overrides
fail explicitly rather than being ignored.

Native status correctly treats the owner PID as the liveness authority because
the native engine has no Chrome child or CDP port. Chromium session records
remain backward-compatible: missing runtime fields are interpreted as
Chromium, and Chromium continues to expose its verified loopback port.

The focused test starts a native owner, sends two commands over the private
socket, verifies the owner and runtime remain stable, and confirms stop removes
the status/socket artifacts. TUI attachment through a named native session,
MCP-to-owner multiplexing, richer native semantic regions/Web IR, workflow
resume, and the remaining Core Web Profile certification gates remain open
issue #40 work.
