# Scroll redraw and return-to-tail verification

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Date: 2026-10-09
Implementation checkpoint: `0a7eec9`.

## Reproduction and fixes

The prior release reports following at the bottom while the last fixture message
is absent from VT cells. Scrolling used the body height even though the header and
controls consume two rows. Text also used the scrollbar column, allowing the
scrollbar to overwrite wrapped text. Reader arrow keys fell through to the composer.

The shared Transcript now exposes its actual text rectangle. Inline, live and
fullscreen navigation clamp against that rectangle; text excludes the scrollbar.
The inline reader leaves the terminal's physical rightmost column untouched to
avoid immediate-wrap corruption at the scroll-region edge. Each reader frame and
composer-cursor restoration are enclosed in synchronized-output boundaries.
Up/Down navigate the reader while preserving the composer draft.

A GPT-6 Luna medium reviewer checked the implementation and identified the live
and fullscreen height mismatch; those callers were corrected and re-reviewed with
no remaining blocker.

## Verification

- 572 workspace tests pass: 485 core, 11 extension API and 76 TUI; one ignored doctest.
- Strict workspace/all-target Clippy passes.
- Fresh locked release build passes. SHA-256:
  `68911fbd9ba1187d9efc3005df7b70ec59d85d1a26bfbcb3af2f797296ef0afb`.
- Six release PTY suites pass: inline-scroll, reader, live, chat, mobile, navigation.
- Inline-scroll covers 65x30, 40x24, 24x16, 20x12 and 20x8: exact cell/style equality
  after wheel roundtrips, actual tail visibility, Up/Down navigation, preserved
  drafts, resize, untouched wrap column, one composer and one app-owned screen.
- Reader checks tool expansion/search, diff navigation and activity at 40x24/20x8.
- Live/chat checks retain streamed replies, reasoning, tools, resize and cancellation.
- The 12,000-record performance gate passes. Its fixture now ends with short marker
  rows so the one-text-row 20x8 reader can expose a marker after initial PageUp;
  measurement conditions and budgets are unchanged.

The workspace release artifact is updated. The separately installed PATH binary
was preserved; use `/home/ubuntu/work/glass-core/target/release/gc` to run this build.
