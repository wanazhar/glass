# Workflow 019-022: stable run identities

Status: completed locally

Every workflow invocation now receives a bounded run ID. The ID is returned in
the run result, copied into its deterministic trace, and retained in exported
checkpoints. Older checkpoints without a run ID remain readable through the
serde default for compatibility; new checkpoints always include the ID.

Resumed suffix executions receive a fresh run ID because they are distinct
invocations. This keeps the original checkpoint identity available while
making each execution and trace independently addressable.
