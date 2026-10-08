# Chat readability

Status: Implemented, independently reviewed, installed and verified in PTY, October 7, 2026.

## Design

Reference: Crush's UI styles (https://github.com/charmbracelet/crush/blob/main/internal/ui/styles/styles.go) and Charm Glamour. Borrow presentation principles, retain GC's Rust implementation and scrollback UI.

The user's mobile screenshot exposes raw bold markers, broken words and dense tool logs. Make the main chat as readable as the Markdown modal.

```text
❯ Explain the workspace [cyan]

  This is a Rust workspace for glass-core.
  Prose wraps at words within a bounded reading column.

  • crates/glass — main application
    Continuation lines align with the item text.

  rust ──────────────────────────
  │ fn main() { ... }
  ───────────────────────────────

  ✓ bash · 0.1s · /details

  773 in · 151 out
  924 tok · 1% ctx
  cost unavailable
  ❯ [input focus]
```

## Contracts

- Main chat, transcript and response modal share Markdown presentation: styled headings/bold/inline code, hanging list indentation, distinct fenced code, readable paragraph spacing.
- User messages use a cyan prompt marker; assistant prose uses neutral text with Markdown accents. No assistant role heading is displayed. Message boundaries use compact spacing, including when provider chunks begin with blank lines; original records remain exact.
- Prose wraps at word boundaries using terminal cell widths. Only oversized tokens and code may split. Retain safe margins on phones and a bounded reading column on desktop.
- Streaming remains responsive. Buffer incomplete Markdown safely, preserve every character in saved logs/copy, flush final incomplete text on completion, cancellation or tool boundary. Do not wait until the entire response completes to show long paragraphs.
- Quiet tool activity with clear success/failure; avoid three noisy rows per successful call. Keep full commands and results in /details and transcript. Do not hide failures or alter execution.
- Never permit provider text to emit ANSI/OSC/control sequences. No terminal capabilities or clipboard commands from Markdown links.
- Input/modal/navigation, headless JSON/plain output, auth, profiles and separate Glass executable remain functional.

## Responsive behavior and verification

Test PTY at 65x30, 40x24, 24x16, 20x12 and existing 20x8. Include long prose, split Markdown tokens across streaming chunks, lists, headings, fenced code, Unicode, final partial lines, cancellation, resize, tool calls and exact copied originals. Verify actual emitted cell widths and word boundaries, not just menu accessibility. Run workspace tests, Clippy and installed-binary PTY suites. No publish or Git commit is authorized.
