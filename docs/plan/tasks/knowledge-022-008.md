---
id: knowledge-022-008
scope: local knowledge management CLI
status: completed
depends-on: [knowledge-022-007]
---

# Objective

Provide explicit local operations for inspecting, exporting, importing,
invalidating, and purging knowledge without launching a browser.

# Delivered

- Added `glass knowledge list|show|stats|export|import|invalidate|purge`.
- Added a per-profile default store path and `--knowledge-store` override.
- Management commands validate profile scope and persistent-storage policy
  before reading or mutating the store.
- Added parser coverage for management commands.
