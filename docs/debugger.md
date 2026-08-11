# Resident debugger service

Glass Dev owns each debug adapter, DAP connection, debuggee PTY, and bounded
state history. Stdio adapters such as debugpy and `lldb-dap` use framed DAP
directly. TCP-only adapters such as Delve use an explicitly configured loopback
address; Glass still starts and stops the adapter process and rejects non-local
TCP endpoints.

## Reverse requests and process ownership

The initialize request advertises `supportsRunInTerminalRequest`. An adapter
`runInTerminal` request is validated, answered, and launched through the Glass
PTY process manager with exact argv values rather than shell interpolation.
The requested working directory must remain under the canonical workspace.
Arguments, environment, process count, output, and protocol queues are bounded.
The resulting process ID, PTY snapshot, output, and lifecycle remain observable
through `glass.debug.inspect` and `glass.debug.processes`. Stopping or dropping
the debugger owner terminates its debuggee process tree.

Unsupported reverse requests receive an unsuccessful DAP response and produce
a `glass/reverseRequest` event instead of deadlocking the adapter. `startDebugging`
is intentionally unsupported until nested session authority and ownership are
defined.

## Configuration and surfaces

Trusted project configuration may define a DAP adapter in `glass.toml`:

```toml
[dap.go]
command = "dlv"
args = ["dap", "--listen=127.0.0.1:38697"]
tcp_address = "127.0.0.1:38697"
connect_timeout_ms = 10000
```

Only a trusted workspace may activate this executable configuration. MCP, Pi,
daemon, and TUI calls share the `glass.debug.*` router. The TUI exposes named
sessions, launch/attach, breakpoint and execution controls, threads, stack,
scopes, variables, watches, console evaluation, events, supervised processes,
and a complete session snapshot. Browser/runtime causal links are recorded by
the development graph and replay service rather than invented by the debugger.

## Certification truth

The local deterministic suite proves framing limits, malformed input,
adapter crash, timeout, unsupported reverse requests, real reverse requests,
loopback TCP, disconnect cleanup, and orphan process-tree cleanup. A real
debugpy lifecycle test covers breakpoints, threads, stack, scopes, variables,
watch evaluation, continue, termination, and cleanup when debugpy is installed.

Linux CI installs pinned debugpy 1.8.21 and Delve 1.27.1 plus the runner's LLDB
package, then runs real source-level lifecycle tests for all three materially
different adapter families. These are native claims only after that job passes;
an environment-gated test that skips because an adapter is absent is not
certification evidence. Delve uses its documented streaming TCP DAP transport;
debugpy and LLDB use stdio.

