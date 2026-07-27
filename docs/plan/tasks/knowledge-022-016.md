---
id: knowledge-022-016
scope: TUI knowledge inspector
status: completed
depends-on: [knowledge-022-015]
---

# Objective

Give the terminal interface a bounded local inspector for persistent browser
knowledge.

# Delivered

- Added `knowledge` and `knowledge show RECORD_ID` TUI commands.
- Displays store stats and safe record summaries, or one validated record,
  without queuing browser work.
- Honors the persistent-profile policy and the configured knowledge-store
  path.
- Added parser coverage for both inspector commands.
