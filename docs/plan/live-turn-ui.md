# Live turn UI

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../INDEX.md) for supported user behavior.

Status: Implemented; final verification and default installation blocked by environment restrictions, October 7, 2026.

## Screen ownership

Native terminal tests confirmed that primary-screen reflow happens before the app receives a resize event, moving old status/input cells into history. Clearing a known row after that event cannot reliably remove decoration without touching conversation. The app therefore owns one alternate screen for its lifetime. Shell primary contents and scrollback are restored on exit. Chat body, status, composer and overlays are application-owned layers; overlays do not nest terminal alternate buffers.

Retain a bounded rendered body cache for viewport repaint/reflow on resize. Original conversation remains in private session records and the scrollable Ctrl-G transcript, with a discoverable way to reach older content. Closing a modal repaints the main viewport exactly once and restores the draft. An application resize cannot introduce decorative rules into shell history. Headless execution uses neither alternate screens nor UI chrome.

## User-visible contract

Fix the supplied screenshots: duplicated composer rules, output/draft collision and Ctrl-G not opening its documented viewer while work runs. Retain rich colors and default activity animation.

```text
  [readable assistant text and scrollback]

  bash · running · 3.2s
  │ cargo test --workspace --locked
  │ [bounded output preview / expand details]

  Thinking · provider reasoning received
  [Ctrl-T /thinking opens live provider-exposed reasoning]

  ────────────────────────────────
  ❯ draft [one owned composer region]
```

- One owner for composer, activity and scrolling output. Separator never accumulates into history on typing, Ctrl-G, resize, tools or viewer close. Never erase prior conversation to remove cosmetic ghosts.
- Ctrl-G opens the transcript both idle and running; Ctrl-G/Esc closes it, preserving draft and current work. Agent execution and provider consumption continue while a viewer is open. Output goes to the right surface and redraws cleanly on close.
- Show actual Bash command text by default in an expanded, styled tool card. Bound previews for phone screens, preserve full details/copy, indicate exit/status/duration; optional compact presentation can be selected without changing execution. Running timer ticks update only the owned status row, never repeat command bodies.
- Forward provider-exposed reasoning events to a provider-backed reasoning indicator and a live scrollable thinking modal, accessible through /thinking and a documented key shortcut during active work. Use only text/summaries returned by the provider; do not invent hidden reasoning or expose opaque/redacted/signature payloads. Distinguish waiting, receiving reasoning, answer streaming, and tool execution. Unsupported/not-yet-returned reasoning shows a clear empty state.
- Existing Anthropic thinking, OpenAI-compatible reasoning_content/reasoning text and Responses reasoning summaries are handled when present. Preserve exact private session logs, original copying, headless JSON/stdin/resume, controls filtering, themes/profiles and account flows.

## Interactions

| Action | Behavior during work |
| --- | --- |
| Ctrl-G | Open or close the live transcript; retain draft and continue execution |
| Ctrl-T | Open or close the live thinking view; retain draft and continue execution |
| Esc in a viewer | Close the viewer and restore the primary surface |
| Ctrl-C | Cancel the active turn, including from a viewer |
| Arrow/Page/Home/End | Navigate viewer content; newly received data preserves explicit scroll and fold choices |
| Terminal resize | Recompute body and chrome ownership, including while a viewer is open |

The thinking view applies its scroll position to rendered content. The transcript retains collapsed entries when live entries update. Commands with many lines get a bounded preview rather than consuming the whole phone viewport. Activity ticks preserve both glyph and color animation.

## Verification

Actual VT projection with real scrolling, save/restore, alternate screens and Unicode widths. PTY reproduces repeated Ctrl-G during long tools, multiple Bash calls, typing/multiline drafts, window growth/shrink/keyboard-height changes, active transcript/thinking open-close and updates, cancellation, reasoning supported/unsupported and provider error. Check no duplicate/lost original text or ghost rules and no output into the wrong screen. Cover 65x30,40x24,24x16,20x12,20x8; retain seven PTY suites, tests and Clippy. Independent review, installed-binary verification. No Git commit/push/publication.

## Provider reference

OpenAI Responses summaries require an explicit `reasoning.summary: "auto"` request for supported reasoning models. Public summary text streams through `response.reasoning_summary_text.delta` and its string `delta` field. A completed summary contains the full text; it must not duplicate already streamed deltas. These summaries are provider output, and do not expose raw reasoning tokens. Verified against the official [reasoning guide](https://developers.openai.com/api/docs/guides/reasoning) and [streaming event reference](https://developers.openai.com/api/reference/resources/responses/streaming-events), October 7, 2026.
