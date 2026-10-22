# Color and motion

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../INDEX.md) for supported user behavior.

Status: Implemented, independently reviewed, installed and verified in PTY, October 7, 2026. Motion is active by default, as requested.

## Overall layout

```text
  glass-core   [colored wordmark / workspace]
  ────────────────────────────────────
  Assistant [accent]

  Styled prose; cyan headings, warm emphasis,
  mint inline code and distinct code panels.

  [animated activity] Thinking · 2.4s
  [settled status]    ✓ bash · 0.1s

  ─── [accent composer divider] ───────
  ❯ draft [keyboard focus]
```

## Contracts

- Enrich the existing Aurora palette with coordinated accent, secondary, success, warning, error, surface and muted colors; keep original theme indices stable. Additional expressive themes are welcome. Main chat, composer, modal selections and progress share theme semantics.
- Animate only active work: provider waiting, tool execution and cancellable modal tasks. Use colorful spinner/shimmer frames and elapsed time. Static prose and completed status remain still.
- Use the existing event/pump loop; avoid stdout-writing background threads, terminal scrolling from ticks, historical text corruption, or input lag. Bound redraws to about 8 frames/second, pause unnecessary idle work. No artificial completion delay.
- Motion is on by default. The user explicitly declined reduced-motion settings: do not add a reduced-motion selector or preference. Make waiting, tools and loading panels visibly animated, with palette cycling and shimmer where safe. Preserve profile theme persistence and existing configurations.
- Respect NO_COLOR through coordinated neutral presentation; colors and motion never affect exact logs, copy, headless JSON, terminal control filtering or process cleanup.
- Composer separators and surfaces must preserve all draft/history/key behavior, terminal scrollback, safe widths and minimum-size dialogs.

## Verification

Test actual changing frames during delayed provider/tool work, no animation after completion, NO_COLOR, no terminal scroll caused by animation, responsive cancel/input/resize, palettes/persistence and bounded cell widths at 65x30, 40x24, 24x16, 20x12 and 20x8. Retain the six existing PTY suites. Independent review before installation. No Git commit/push/publication.
