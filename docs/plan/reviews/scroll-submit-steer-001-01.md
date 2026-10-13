# Scroll submission and Chat Completions steering

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

2026-10-09

## Changes

- Ordinary paused-reader Enter submits a composer draft and returns to live view. Empty Enter closes an unselected reader; explicit search Enter remains next-match. Card Enter remains available when the composer is empty.
- Busy Enter retains its correction in the REPL queue and interrupts the Chat Completions receive loop. Received prose is retained; incomplete and unexecuted tool calls are discarded. The next request uses existing conversation history and the durable user correction, before queued follow-ups.
- Automatic checkpoints skip plain directories without initializing Git. Explicit Git actions retain errors; other checkpoint failures remain visible.
- Alt+Enter remains queued follow-up delivery. This is a client-side Chat Completions fallback, not native Responses WebSocket steering. It cannot undo completed commands or guarantee immediate remote stream termination.

## Verification

- Workspace tests: 487 core, 11 mod API, 76 TUI pass (574 total); one ignored doctest.
- Strict all-target workspace Clippy passes; release build passes.
- New release PTY smoke-scroll-submit passes at 40x24 and 20x8 in a plain directory. It proves the correction starts a second request while the first stream is held, retains original prompt and earlier history, records each user prompt once, and produces a durable reply without Git fatal warnings.
- Release smoke-inline-scroll and smoke-reader pass. Reader test clears its composer before explicit card Enter actions and restores the draft afterward.
- Debug smoke-live and smoke-chat pass at five terminal sizes through 20x8.
- GPT-6 Luna medium review found no concrete steering blocker.
- Existing installed PATH binary was preserved. Updated executable: target/release/gc.
