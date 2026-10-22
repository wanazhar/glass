# Slash palette Escape restoration

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Date: 2026-10-09.

Escape already closed palette state, but a managed palette clears and paints the application screen. The normal composer repaint left its body rows visible. Restore the managed surface when the palette transitions from active to closed; an active reader then uses its existing overlay restoration path.

Regression coverage checks Escape from both the slash menu and an unmatched filter, asserting that palette header and empty-results text disappear. Existing draft preservation semantics remain covered.

Verification: 601 workspace tests passed, one ignored doctest. Debug polish PTYs passed at 80x24, 40x12, 24x10 and 20x8. Reader PTYs at 40x24 and 20x8 and existing feature PTYs passed. One GPT-6 Luna medium review found no blockers. Release polish PTYs also passed at all four sizes, including resize and draft preservation.
