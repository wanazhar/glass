# Conversation scrollbar range

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Date: 2026-10-09.

The conversation followed its real tail, but ScrollbarState received total wrapped lines as its number of possible positions. Ratatui interprets the last position as content_length minus one; the transcript's last legal offset is total lines minus visible text rows. This mismatch left the thumb above the bottom, especially when history was only slightly taller than the viewport.

Use max scroll offset plus one as content_length, and explicitly pass the same text viewport height used by tail positioning. This is constant-time arithmetic with no additional parsing, allocations or history traversal.

Regression checks render short, medium and long histories at 80x24, 40x19, 20x8 and 19x4, asserting the thumb touches both ends. Existing 19x3 tail rendering remains covered; views with no room between scrollbar arrows render no thumb. Offline PTY checks explicitly jump to absolute top and End, checking scrollbar track endpoints at 40x24 and 20x12, plus full reader interaction coverage at 20x8.

Verification: 602 workspace tests passed, one ignored doctest; strict Clippy passed. Debug and release reader PTYs passed at all three sizes. GPT-6 Luna medium review confirmed the scrollbar contract and identified tiny-screen test assumptions, corrected before final verification.
