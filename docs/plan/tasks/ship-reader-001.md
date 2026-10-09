---
id: ship-reader-001
status: complete
scope: shared transcript search and reading anchors (items 8-9)
---

Implement items 8-9 of ../ship-next-20.md in shared transcript.rs/layout.rs,
reader.rs and live.rs only. Own those paths; do not edit main/agent/session.
Use /search command integration later, but Ctrl+F focused reader works now.
Search editable query, next/previous match, jump to command/output/text matches;
Esc cancels search before closing reader. Keep draft separate. Stable entry/line
anchors survive width changes, overlays and append; reopen live alternate reader
retains position. Provide exact main hooks to root for later integration.
Add tests and smoke-reader.py for mobile/live projection; preserve current colors
and motion. Existing Alt/Ctrl-G/Ctrl-T behavior remains compatible.
