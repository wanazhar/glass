---
id: semantic-020-002
scope: deterministic landmark and page classification
status: completed
depends-on: [semantic-020-001]
---

# Objective

Produce the first semantic observation from existing bounded page and
accessibility evidence without weakening the legacy observation path.

# Delivered

Added `BrowserSession::semantic_observe` and a deterministic classifier for
ARIA landmark regions and a small set of high-confidence page signatures.
Unknown pages remain `generic`/`unknown`; every region carries bounded
evidence and a revision-scoped expansion handle. Existing interactive counts,
truncation, and omission signals are retained.
