# Inline conversation scrolling

Status: Implemented; offline PTY verification completed, provider-backed gates outstanding (October 7, 2026).

## Layout

```text
┌ conversation viewport ────────────────────┐
│ Older conversation and expanded tools    │
│ [scrollbar / stable reader position]     │
└─────────────────────────────────────────┘
  ↓ Back to bottom · Esc · 3 new updates
  [animated provider/tool phase]
──────────────────────────────────────────
❯ unsent draft [pinned, editable]
```

## Contract

- Scroll the main chat body in place using PageUp/PageDown and mouse-wheel events when supported by the terminal. Scrolling is available idle and during provider/tool work. Keep the composer, divider and activity status pinned and visible.
- Ctrl-B/Ctrl-F provide page navigation from mobile terminal key bars. Ctrl-Home reaches the oldest retained entry. Plain Up/Down keep their editor/history behavior.
- Scrolling up disables automatic following. New output continues to be consumed, saved and counted without pulling the reader to the bottom. Escape or a discoverable End/Ctrl-End action returns to the latest content; preserve drafts and ordinary cursor/history editing semantics. Returning to the bottom restores automatic following.
- Reuse the existing transcript/layout/Markdown representation for original conversation and tool text, rather than flattening or reverse-engineering ANSI output. Maintain the one application-owned alternate screen; no nested terminal screens.
- Ctrl-G remains the dedicated transcript viewer, and Ctrl-T remains the provider-exposed reasoning view. Ordinary modals restore the prior inline reader state and draft. Busy Ctrl-C still cancels work.
- Mouse capture is scoped to the app lifetime and correctly restored for exit and external editors. Supported keyboard controls remain sufficient over SSH/mobile terminals that do not emit wheel events.
- Full retained session history is reachable. Narrow views wrap rich text and keep a short back-to-bottom/unread hint reachable at 20x8. Show a compact status form when height demands it. Theme colors, default motion, NO_COLOR, sanitization, exact copy and headless behavior remain intact.

## Verification

Offline PTY with a synthesized private long session: scroll, pinned draft editing, wheel events, resizing at 65x30/40x24/24x16/20x12/20x8, return-to-bottom, modal restore, one divider, one alternate-screen entry, and shell restoration. Unit contracts for following versus paused reading and incoming updates. Live provider continuation, unread updates and cancellation require the provider-backed PTY gate; current localhost restrictions must be reported without substituting a pass. Independent review before private release installation.
