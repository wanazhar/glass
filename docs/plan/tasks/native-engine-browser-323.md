# Native batch and workflow surfaces (323)

Status: implemented locally in the native runtime.

This slice completes native declarative batch/workflow execution across the
remaining first-class browser surfaces. The native CLI and MCP adapters now
parse and policy-check typed batches, execute revision-guarded navigation,
input, form, wait, observation, screenshot, script, and dialog steps, and
return the shared `BatchOutcome` contract. Native workflows use the same
typed batch driver for locator actions and the native intent resolver for
intent steps. Preconditions, conditional branches, bounded repetition,
pre-dispatch retries, effect markers, postconditions, terminal proofs,
bounded typed outputs, and budget statuses are retained in the shared
`WorkflowRunResult` and `WorkflowTrace` schemas.

Native workflow checkpoints use the existing bounded schema and include the
native target/frame route, title, URL, revision, step histories, execution
IDs, branch decisions, and intent evidence. Resume validates the definition
hash, checkpoint shape, route identity, state transitions, and safe
pre-dispatch retry boundary before executing only the uncommitted suffix.

The persistent native owner now has a typed workflow IPC operation for run,
checkpoint export, and resume. The browser TUI uses those operations when it
is attached to a named owner and calls the equivalent runtime methods for an
in-process native session. The owner remains the authority for policy,
profile, storage, viewport, revision, and workflow execution; a TUI client
does not create a second engine or stop the owner when it exits.

Focused validation for this slice includes default and no-default
`glass-browser` test-target checks, the persistent-owner test covering execute,
control, MCP, workflow, checkpoint, PID retention, and cleanup, formatting,
and the native policy/runtime paths. The remaining issue #40 gates are native
semantic/Web IR completion, complete Core Web Profile conformance, recovery
and cancellation certification, cross-platform packaging, and native-only
release evidence.
