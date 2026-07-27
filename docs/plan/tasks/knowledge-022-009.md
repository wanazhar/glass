---
id: knowledge-022-009
scope: MCP knowledge management tools
status: completed
depends-on: [knowledge-022-008]
---

# Objective

Expose bounded persistent-knowledge inspection and lifecycle operations over
MCP without starting Chrome or treating stored knowledge as mutation
authorization.

# Delivered

- Added `knowledgeList`, `knowledgeShow`, and `knowledgeStats` read tools.
- Added `knowledgeInvalidate` and `knowledgePurge` lifecycle tools.
- Kept knowledge dispatch before browser-session creation and gated it behind
  the persistent-profile policy capability.
- Added explicit schemas, bounded identifiers, and lifecycle-state enums to
  the advertised tool contract.
- Added coverage for the expanded tool list and a valid knowledge request that
  leaves the browser session unopened.
