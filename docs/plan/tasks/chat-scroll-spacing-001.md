---
id: chat-scroll-spacing-001
status: in-progress
scope: initial scrolling and compact message separation
---

## User contract

Scrolling must start directly from the normal conversation, idle or busy, using wheel events, PageUp or Ctrl-B. Preserve the draft, cursor, paused reading position, palette layering and dedicated viewers. Terminal-local touch gestures that emit no input cannot be handled by the app; document actual controls and verify the binary being launched.

Remove the visible Assistant heading from streamed and retained conversation views. Render user messages with the existing cyan prompt marker and emphasize their text; assistant prose stays neutral with existing Markdown colors. Use one compact boundary between messages, not an empty heading and multiple blank rows. Keep streaming, tool transitions, small-screen wrapping, NO_COLOR, original records and headless output intact.

```text
❯ user message [cyan]

Reply text [neutral, styled Markdown]
  continued wrapped text
───────────────────────────
❯ draft
```

## Gates

Independent PTY at 65x30/40x24/24x16/20x12/20x8: wheel starts reader without prior key, keyboard starts reading, paused drafts and modal restoration, compact user/reply spacing, no Assistant heading, retained Markdown and one owned screen. Add focused tests for input routing and first streamed text/tool transitions; use direct Painter fixture if provider listeners remain denied. Verify a rebuilt release inside the workspace or /tmp and report default installation permissions accurately.

## Findings and implemented fixes

Both input loops previously discarded wheel events until the inline reader was already open. First wheel events now open the reader and dispatch the same event; palette and dedicated-view ownership are respected. The shared rendering path removes the role heading and suppresses leading blank response chunks without losing first-line indentation or changing saved text. User prompts use cyan `❯`; retained assistant entries have no role prefix. Prompt/reply and retained-entry boundaries use one blank row. Prefix buffering is bounded and safe for Unicode whitespace.

Focused Painter tests (9), layout tests (16), transcript tests (18), all-target Clippy and the five-size shipping PTY smoke pass. Independent VT projection of captured real rendering bytes and first-input wheel/keyboard PTY pass at all five sizes. Live responding-provider behavior remains blocked by sandbox networking; the captured rendering fixture and synthetic history checks are not presented as provider-backed verification.

The supported offline installer rebuilt the frozen production source at `/tmp/gc-final-install/bin/gc -> glass-core`; log `/tmp/gc-chat-spacing-install-final.log`, executable SHA256 `8df0229c8678f96e6f74ed2635f8b9858258a85b631c545aab44f4e6c085ea53`. The separate Glass executable remains unchanged. The default user install remains outside the writable roots.
