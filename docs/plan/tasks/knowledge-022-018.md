---
id: knowledge-022-018
scope: knowledge store path safety
status: completed
depends-on: [knowledge-022-017]
---

# Objective

Keep profile-derived knowledge paths within the validated profile namespace.

# Delivered

- MCP startup now applies the same profile-name validation used by local
  profile and knowledge CLI commands.
- Path-like or otherwise invalid profile names are rejected before the MCP
  session or knowledge store is opened.
