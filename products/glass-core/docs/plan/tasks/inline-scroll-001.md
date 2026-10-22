---
id: inline-scroll-001
scope: scrollable main conversation with pinned input
status: in-progress
depends-on: [composer-greeting-001]
---
# Inline scroll

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

Implement ../inline-scroll.md. Reuse transcript/layout state, integrate idle and active keyboard pumps, preserve owned surface boundaries and current rendering/security/headless behavior. Provide offline PTY proof across five sizes, unit state contracts and independent review. No default installation or publication under the current restricted environment. Provider-backed verification remains explicitly outstanding if localhost is still denied.

## Delivery evidence

Implementation and independent offline PTY review pass at 65x30, 40x24, 24x16, 20x12 and 20x8. Verified scrolling, pinned editing and insertion cursor, bracketed paste, palette restoration, follow-at-tail, wheel navigation, no-key idle resizing, quiet idle output and terminal restoration. Focused transcript (18) and surface (4) tests and scoped Clippy pass. See ../reviews/inline-scroll-001-01.md. Active provider streaming, unread updates and cancellation remain outstanding because this sandbox denies loopback listeners.

Final supported offline release installation succeeded at `/tmp/gc-final-install/bin/gc` (relative symlink to `glass-core`); the five-size shipping PTY smoke passed against that exact installed binary. Logs: `/tmp/gc-inline-private-install-final.log`, `/tmp/gc-inline-installed-smoke.log`. The default user install is unchanged because it is outside this environment's writable roots. The separate `glass` executable checksum remains unchanged.
