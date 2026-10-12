# Welcome-back recap

A local, display-only catch-up appears when opening or switching to a saved
conversation. `/recap` reopens it in the existing scrollable, resize-aware modal;
Enter/Escape dismisses it without editing the composer. Empty conversations show
an explanatory message only when requested.

The card distinguishes the latest request, latest reply excerpt, saved blocker,
and an unanswered request. A reply excerpt is not treated as proof of completion.
A successful later reply clears an older error from the recap. The card uses the
active theme's accent, body, warning, secondary and muted tones, with word wrapping
and bounded excerpts for small SSH terminals. Transcript scrolling retains the
same recap text. It is removed from the live projection when a new turn starts.

No provider request, reasoning generation, tool execution, or prompt insertion is
performed. Extra token cost is zero. The recap cache lives next to the session as
`*.recap.json`, uses private file permissions and atomic replacement, and validates
version, session length, modification time, device, inode and change time. Invalid
or unavailable caches fall back to local reconstruction. Cache writes are best
effort and cannot prevent resuming. Context edits use the effective history.

This first version deliberately quotes saved evidence rather than generating a
new summary. It triggers on resume/session selection, not terminal focus events.
A future generated recap can be opt-in with explicit accounting and the same
UI-only boundary; it is not part of this implementation.

Verification: six focused Rust regressions cover scope, empty sessions, Unicode,
errors/recovery, cache invalidation and model-history isolation. Release PTYs in
`scripts/smoke-recap.py` exercise first/repeated resume, modal open/close, composer
input, cache creation and zero provider requests at 120x36, 80x30, 40x24 and 20x8.

Final verification: 841 workspace tests pass (one unrelated doctest ignored),
strict all-target workspace Clippy passes, and the optimized release build passes
all four recap PTYs plus existing reader regressions. Luna medium review identified
a cache identity edge case; fixed and guarded with a same-length replacement / 
preserved-mtime regression. Isolated performance budgets pass: prompt startup
worst median 7.0 ms, visible key 0.072 ms, redraw 0.323 ms, and 12,000-record reader
132.0 ms. Report: `/tmp/gc-recap-performance-isolated.json`. The earlier run during
release compilation exceeded the reader budget; the idle-compiler rerun passed.
