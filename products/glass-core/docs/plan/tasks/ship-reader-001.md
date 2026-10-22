---
id: ship-reader-001
status: complete
scope: shared transcript search and reading anchors (items 8-9)
---
# Delivery: reader

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

This task implements roadmap items 8–9 in `transcript.rs`, `layout.rs`,
`reader.rs`, and `live.rs`. Main-loop integration is handled separately.

The reader supports an editable search query, previous/next matches, and jumps
to matching commands, output, or prose. Esc closes search before closing the
reader. Draft state remains separate. Entry/line anchors survive resizing,
overlays, and appended content; reopening a live reader retains its position.

Provide integration hooks for `/search`, and verify mobile/live projections with
unit checks and `smoke-reader.py`. Preserve the existing colors, motion, and
Alt/Ctrl-G/Ctrl-T controls.
