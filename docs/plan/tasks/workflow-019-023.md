---
id: workflow-019-023
scope: per-step workflow evidence envelope
status: completed
depends-on: [workflow-019-022]
---

# Objective

Expose the evidence fields required to interpret a workflow step without
reading raw CDP logs.

# Delivered

- `dispatchAcknowledged` distinguishes a pre-dispatch failure from an action
  that reached the batch executor.
- `effectObserved` and `postconditionVerified` are populated independently.
- `retrySafe` reflects the declared transaction class and remains false for
  unknown and non-idempotent steps.
- `previousRevision` and `currentRevision` retain the bounded revision
  boundary observed around the action.
- Older serialized step records remain readable through serde defaults.

The fields are evidence summaries, not claims of external rollback or server
transactionality.
