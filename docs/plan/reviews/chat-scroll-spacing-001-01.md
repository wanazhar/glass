# Review: chat-scroll-spacing-001

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Status: **PARTIALLY VERIFIED — LIVE PROVIDER GATE BLOCKED** (October 7, 2026)

## Source review

- The idle main loop opens the inline reader for its first wheel-up or wheel-down event, then passes that same event through. PageUp and Ctrl-B retain their existing reading paths. The busy input drain likewise opens the reader from the live transcript snapshot when no dedicated view owns input.
- Dedicated transcript/reasoning viewers and an active palette retain input ownership; wheel events are not stolen from them.
- Retained user entries use the cyan bold `❯ ` marker. Assistant entries have no role heading and trim leading blank display rows without modifying their stored text. The layout inserts one blank row between entries and wraps using the current width.
- The Painter discards only the first response's leading whitespace rows, preserves bounded Unicode prefix handling and code indentation, and shares a transition function for text/tool boundaries. It emits a single blank boundary before the response and after a tool transition. Markdown styling remains in the stream renderer.

## Verification completed

- Independent offline PTY against the exact private installed binary `/tmp/gc-final-install/bin/gc`: `/tmp/gc-chat-spacing-wheel-final-installed.log`. At 65×30, 40×24, 24×16, 20×12 and 20×8, first-input wheel-up opened a paused reader; first-input wheel-down opened in follow mode; the pinned composer and draft remained visible; the transcript had no Assistant heading and used one alternate-screen entry. PageUp and Ctrl-B also opened reading from normal chat at every size.
- Independent offline inline-reader PTY against that same binary: `/tmp/gc-chat-spacing-inline-installed.log`. All five sizes passed first PageUp, pinned draft and cursor position, bracketed paste, palette cancellation/restoration, tail-follow and wheel behavior. An idle resize from 40×24 to 20×8 and back redrew without a key.
- Dedicated viewer switch and exit checks against the same binary passed: `/tmp/gc-chat-spacing-switch-installed.log` confirms Ctrl-G/Ctrl-T return to the inline reader with one alternate screen; `/tmp/gc-chat-spacing-exit-installed.log` confirms exit restores alternate-screen and mouse-capture modes.
- Painter ANSI capture test passed (`/tmp/gc-chat-spacing-capture-test.log`). A byte-preserving VT projection of the production-shared prompt/text/tool transition renderer passed at all five sizes (`/tmp/gc-chat-spacing-vt-review.log`): one prompt-to-reply blank row, one tool-end-to-final-reply blank row, Markdown styling present, and no Assistant heading. The projection reads raw bytes to preserve CR erase sequences.
- Focused tests passed: 9 Painter tests (`/tmp/gc-chat-spacing-painter-tests.log`), 16 layout tests (`/tmp/gc-chat-spacing-layout-tests.log`), and 18 transcript tests (`/tmp/gc-chat-spacing-transcript-tests.log`).
- The final private release install completed successfully (`/tmp/gc-chat-spacing-install-final.log`). The existing default `glass`/`glass-core` installation was not replaced.

## Outstanding

- Loopback provider listeners are unavailable in this environment. First-wheel routing while a provider is actively streaming, updates arriving while the reader is paused, and continued provider/tool behavior therefore remain unverified against a live or mock provider. Source inspection and the routing predicate tests are not a substitute for that runtime gate.
- This review's Painter capture tests the shared renderer transitions directly, while the retained-history and wheel behavior use a real PTY with a private synthetic session. No provider response or provider request was made.

No claim is made about live-provider streaming or update behavior.
