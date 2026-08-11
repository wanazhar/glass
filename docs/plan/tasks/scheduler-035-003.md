# scheduler-035-003 — autonomous verified task DAG and workspace actors

Status: Complete locally on 2026-08-11. No remote mutation performed.

## Outcome

Glass now separates task, agent, and Pi session state. `TaskScheduler` owns a
bounded DAG and automatically allocates an agent, sends the prompt, interprets
settled events as a transition into verification, retries failures, completes
verified tasks, and wakes dependents. Failed prerequisites block descendants
unless a human deliberately overrides them.

Every durable workspace is now owned by a bounded actor queue. The daemon
registry borrows only long enough to look up/open/list/remove actor handles;
tool execution never holds a global workspace map lock. Actor ticks advance
tasks without client polling and resource state survives client disconnects.

## Evidence

- deterministic fake-agent tests prove automatic prompt dispatch and the task
  state machine without a paid model call;
- a real trusted command verifier proves retry then failure/block propagation;
- LSP evidence proves an agent claim waits for governed verification;
- task APIs are available through the shared router and full TUI layouts;
- a real shell-kernel concurrency test proves a one-second operation in
  workspace A does not delay inspection of workspace B beyond 300 ms;
- the existing reconnect test still proves PTY, kernel, optional Pi, and
  browser resources survive disconnected clients.

## Limits carried forward

Native Windows transport is Gate 6, not part of this Unix actor checkpoint.
Streaming event subscriptions and broader subsystem-specific parallelism are
completed with the cross-platform daemon contract rather than represented as
already shipped here.
