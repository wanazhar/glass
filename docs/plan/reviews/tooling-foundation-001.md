# Tooling foundation and turn recovery review

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Status: verified.

## Delivered scope

- Revision-checked continuation of large UTF-8 file reads, with exact byte cursors.
- Proposed/applied/unchanged file mutation receipts and dry-run previews.
- Strict edit operation validation and explicit shell argument errors before execution.
- Durable state clears and extension-mediated state changes with rollback on persistence failure.
- Reliable process-group timeout cleanup when descendants ignore TERM.
- Main turns without the implicit 24-round cutoff; assigned teammate limits retained.
- Answer recovery with a stable cached system prefix, restored after successful tool progress.
- Bounded compaction requests and durable extractive fallback when summarization fails.
- Typed tool cards rendered consistently while scrolling, with explicit expansion and output availability hints.

## Review

Luna medium reviewed the implementation. Findings addressed: missing expected revisions on byte continuations; nested state persistence; explicit main limits overriding the constructor; running tools styled as completed; duplicate expanded previews; missing spill/truncation hints; and successful truncated output styled as failed.

## Evidence

- Full workspace suite: 546 core + 13 mod API + 89 TUI tests pass (648 total); one ignored doctest.
- Clippy across all targets passes with warnings denied.
- Actual release PTYs pass at 80×30, 40×24 and 20×8: 28 tool rounds, reasoning-only recovery with stable system prefix, failed-summary fallback, scrolling and follow-up submission.
- All 15 file CLI edge cases pass outside Git; native file/job PTYs pass at the same three sizes, including previews, validation, background input/poll/stop and durable state clear.
- Release binary SHA-256: `968db4bd4690981dbf9cb821a7fad4bf210fa58a2ac0ada55324b157ae55c6ed`.
- Performance smoke passes with 12,000 history records: worst median key visibility 0.23 ms, redraw 0.50 ms, reader frame 112.92 ms; measured history child peak RSS 3,492 KiB.
- Native helper medians: 100 KB read 2.03 ms, 1 MB read 6.33 ms, 8 MB read 18.99 ms, 1 MB edit 25.73 ms. Ordinary Python writes spawn no checker.
- Installed `/home/ubuntu/.cargo/bin/gc` remains a symlink to the older installed `glass-core`. The verified binary is `target/release/glass-core`; active sessions were not interrupted.

## Limits

A provider can still ignore reduced reasoning and return no answer twice consecutively; GC reports that genuine failure rather than repeatedly spending tokens. Recovery preserves existing tool results and does not replay them itself, but cannot guarantee that a model will never request a completed action again. Extractive compaction preserves bounded excerpts and the original append-only log; it is less expressive than a successful model summary. Provider cache hit rate is controlled upstream; stable prefix tests establish cache eligibility, not a guaranteed hit percentage.

File replacement retains mode bits but does not claim ownership/xattr preservation or atomic compare-and-swap against arbitrary external writers. Advisory locks serialize cooperative GC writers. This delivery begins roadmap items 38–43; it does not complete the remaining roadmap, background-process work or full diagnostics expansion.
