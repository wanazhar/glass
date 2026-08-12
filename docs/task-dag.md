# Autonomous task DAGs

Glass tasks are scheduled units above agents and Pi sessions:

```text
Task (goal, prompt, policy, verifier, evidence)
  -> Agent (role, authority, budget, worktree)
  -> Pi session (conversation, model, thinking, tools)
```

Creating a task is an execution request, not metadata registration. Once every
dependency has verified successfully, Glass allocates an agent, waits for its
native SDK session, sends the task prompt automatically, observes the settled
event, runs verification, and either succeeds, retries, or fails the task.
Verified success wakes dependents. Failure and cancellation mark descendants
blocked; a local human may explicitly override that state.

If the caller does not supply a verifier, Glass infers a project verifier at
creation time: locked Cargo workspace tests, `npm test`, `python -m pytest`, or
`go test ./...` when their project markers exist, and a required Git change as
the conservative fallback. Settle-only acceptance is available only through
the explicit `settled` policy; it is never the implementation default.

## States and controls

Tasks move through `queued`, `ready`, `running`, `waiting`, `verifying`, then
`succeeded`, `failed`, or `cancelled`. `paused` and `blocked` preserve an
explicit reason and require resume/retry/override actions.

The product UI uses `✓` for verified success, `◇` for deliberately
settled/unverified completion, `×` for failed/cancelled work, `!` for blocked
or ambiguous work, and `●` while running or verifying. Evidence and the exact
verification policy remain visible beside that glyph.

Each snapshot includes stable task and assigned-agent IDs, title, goal, full
prompt, dependencies, worktree, model/thinking and authority policy, runtime,
event, and token budgets, verifier, retry policy, timestamps, bounded evidence,
and last failure. TUI actions support create/create-after, inspect, pause,
resume, cancel, retry, reassign with model/thinking changes, blocked override,
and evidence inspection/submission in desktop, compact, and phone layouts.

The shared router exposes:

```text
glass.task.create              glass.task.list
glass.task.get                 glass.task.pause
glass.task.resume              glass.task.cancel
glass.task.retry               glass.task.reassign
glass.task.override-blocked    glass.task.evidence
```

CLI/MCP/Pi clients use these through the same tool contract. Untrusted
workspaces may inspect existing tasks but cannot create, control, or supply
verification evidence.

## Verification

A settled assistant response is evidence, not proof. Policies include:

- exact exit status from a bounded trusted command;
- LSP diagnostic budgets;
- resident browser workflow assertions;
- semantic-regression budgets;
- debugger assertions;
- Git change/cleanliness constraints;
- trusted project verifiers; and
- bounded `all` compositions.

Command verification and resident Git/browser/semantic/custom evidence are
collected automatically. LSP/debugger evidence can be supplied only by their
governed resident service path and is checked against the policy fields rather
than accepting a bare “passed” claim. Evidence is capped per task. Failed
verification follows the configured retry/backoff policy.

## Durable scheduling

Each `glassd` workspace actor ticks its scheduler every 50 ms even with no
connected client. Client disconnects therefore do not pause task/agent/session
state. The daemon registry stores only bounded actor handles; a slow task or
tool in workspace A does not hold a global workspace lock or delay workspace B.

## Verification

```bash
cargo test -p glass-dev tasks::tests -- --nocapture
cargo test -p glass-dev daemon::tests::workspace_actors_do_not_serialize_unrelated_long_operations -- --nocapture
```

The deterministic scheduler backend proves dispatch, dependency wakeup,
verification waiting, retries, and descendant blocking without consuming a
model request. The daemon test runs a real one-second shell-kernel operation in
one workspace while another actor remains responsive under 300 ms.
