---
id: knowledge-022-014
scope: MCP knowledge-backed intent resolution
status: completed
depends-on: [knowledge-022-013]
---

# Objective

Expose the fresh-only and historical-evidence intent boundary through MCP
without making persistent knowledge implicit for existing callers.

# Delivered

- Advertised and parsed `resolveIntentWithKnowledge` with explicit scope
  dimensions.
- Reused the current guarded resolver and fresh observation path.
- Gated store consultation with the persistent-profile policy capability.
- Added parser and tool-advertisement coverage while preserving the existing
  `resolveIntent` behavior.
