---
id: chat-readability-001
scope: crates/glass-tui Markdown and crates/glass interactive chat
status: done
depends-on: []
---

## Objective

Implement the user's requested Charm/Crush-inspired chat readability redesign, including streamed long prose and compact tool feedback on mobile terminals.

## Context

- ../chat-readability.md: approved design scope inferred from the user's explicit request.
- https://github.com/charmbracelet/crush/blob/main/internal/ui/styles/styles.go: visual reference.
- https://github.com/charmbracelet/glamour/blob/master/styles/dark.json: visual reference.

## Paths

- crates/glass-tui/src/markdown.rs and related rendering helpers.
- crates/glass/src/main.rs Painter integration and presentation tests.
- scripts/smoke-mobile.py, a dedicated Markdown PTY suite, README.md, CI smoke list as needed.

## Integration

Provider Progress::Text -> Painter -> shared Markdown stream -> ANSI scrollback output.
Session original records -> transcript/response modal -> shared Markdown styled word wrapping.
Tool events -> compact status, with originals still available in details/logs.

## Verification

Workspace Rust tests and Clippy. Real PTY across mobile widths, desktop, resize, chunk-split formatting, Unicode, streaming-before-finish, long prose, fences, tool failure, cancellation, exact originals. Independent reviewer inspects actual rendering and security, including controls and maximum pending buffers. Root installs final release; reviewer verifies installed gc. No commits/push/publication.
