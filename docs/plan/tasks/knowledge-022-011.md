---
id: knowledge-022-011
scope: MCP fresh semantic knowledge observation
status: completed
depends-on: [knowledge-022-010]
---

# Objective

Expose an explicit semantic observation mode that always collects current
browser evidence before consulting local knowledge.

# Delivered

- Added the `observeKnowledge` MCP tool with fresh-only and assessed modes.
- Added explicit profile, locale, tenant, browser, and policy scope inputs.
- Kept fresh-only requests independent of the local store, including a
  corrupt or unavailable store.
- Kept assessed results read-only and non-authorizing; current observation
  evidence remains the only source for later browser actions.
