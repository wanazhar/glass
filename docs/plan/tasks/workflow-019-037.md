---
id: workflow-019-037
scope: resumable workflow evidence
status: completed
depends-on: [workflow-019-036]
---

# Objective

Keep resumed workflow results compatible with the original definition and
preserve enough bounded, redacted step evidence for subsequent export and
offline trace replay.

# Delivered

Checkpoints now retain bounded state histories, execution identifiers, dispatch
flags, revision boundaries, and branch decisions. Resume merges the committed
prefix back into the new run result while assigning the resumed invocation a
new run ID. Legacy checkpoints without the additional evidence fields remain
readable and use a conservative committed-state history when replayed.
