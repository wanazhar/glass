# debugger-035-006 — DAP completeness and native certification

Status: Implementation complete locally on 2026-08-11. Native three-family CI
evidence awaits an authorized remote run.

Glass now dispatches DAP reverse requests while waiting for ordinary responses.
`runInTerminal` uses an exact-argv, workspace-confined Glass PTY whose output,
PID, state, and cleanup remain owned and observable. Unsupported reverse
requests receive explicit unsuccessful responses. Stdio and owned loopback TCP
transports cover debugpy/LLDB and Delve without presenting a fixture as an
adapter certification.

Session snapshots retain adapter state, capabilities, source breakpoints,
watches, bounded events, and debuggee processes. Router and TUI actions expose
the complete inspection surface.

Local protocol tests cover malformed/oversized frames, crash, timeout,
unsupported and supported reverse requests, loopback TCP, disconnect, and
orphan cleanup. CI installs pinned debugpy and Delve plus LLDB and executes real
breakpoint-to-stack-to-continue lifecycles for all three adapter families.

