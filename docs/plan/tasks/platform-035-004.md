# platform-035-004 — native local daemon transports

Status: Implemented locally on 2026-08-11; Windows native execution awaits the
existing remote Windows CI runner. No push or remote mutation performed.
Status: Historical checkpoint; superseded by the current 0.3.14 source/release evidence.

## Outcome

The durable workspace daemon now has two intentionally local transports:

- Unix-domain sockets with mode `0600` and Linux peer credential checks;
- Windows byte-mode named pipes with Glass-owned endpoint validation,
  first-instance collision protection, remote-client rejection, a per-user
  endpoint derived from LocalAppData, and the same private 256-bit token.

Both use the same bounded newline JSON protocol, client quota, workspace actor
registry, reconnect identity, status/doctor data, and request implementation.
Named pipes disappear with their owner; stale status is rejected by PID checks
and replaced on the next start. Windows stop terminates the exact status-bound
PID, which closes the existing Job Objects and therefore reaps owned process
trees.

## Native evidence contract

The Windows-only integration test starts the installed `glass` test binary,
checks status and endpoint identity, opens a workspace, executes a confined
file tool, reconnects through a fresh pipe client, rejects a non-Glass pipe
name, stops the daemon, and cleans its isolated data. CI has a named
Windows-only step for this test in addition to the full Windows suite.

The source checkout has both MSVC Rust targets installed, but this Linux host
does not have the MSVC C toolchain required by transitive `ring`; local cross
checking stops honestly at that external compiler boundary. Only the native
Windows CI result may be recorded as Windows certification.
