# Review: composer-greeting-001

Status: **PARTIALLY VERIFIED — PROVIDER GATE BLOCKED** (October 7, 2026)

## Source review

- Successful prompt dispatch records the user message once, echoes it to the body, then clears the bottom three owned rows and paints a fresh empty focused editor before provider work starts.
- Provider/auth factory preflight errors restore the prompt into the editor before continuing, so the text stays available for correction.
- During active work, Enter steering redraws the cleared editor before the handoff; Alt+Enter follow-up marks the composer dirty so the common render path clears it. At turn completion, the active draft is copied back to the outer editor.
- The base prompt describes greetings and ordinary conversation as direct responses without workspace inspection, while coding/workspace requests still direct inspection and verification. It does not use keyword routing or prohibit tools categorically.
- The divider helper now draws `width - 1` cells, avoiding terminal autowrap.

## Verification completed

- `cargo test -p glass-core --locked surface::tests`: 4 passed, including the cleared composer region, wrapped input rows, and full-width divider cases at widths 20, 24, 40 and 65.
- `cargo test -p glass-core --locked base_prompt_keeps_conversation_separate_from_workspace_work`: passed.
- `cargo build -p glass-core --locked`: passed on the reviewed source.
- PTY with no provider listener, 40×24 and 20×8: `hello` was echoed once; the `Waiting` phase preceded the failed connection; the focused composer was blank after dispatch; the single session log contained one user record. The sandbox denied the connection with `Operation not permitted`, so no model response occurred.
- Missing-key preflight PTY at 40×24: the provider-not-ready modal returned to the composer with `hello` retained.
- Busy no-listener PTY at 40×24: Alt+Enter cleared queued text, a later unsent draft remained visible after the failed turn, the original prompt appeared once in the session, and queued text was not prematurely duplicated. Enter steering also cleared its composer before handoff.
- Full-width divider assertions passed at the initial 40×24 and 20×8 sizes.
- Independent offline PTY resize now passes with the updated geometry polling: typed draft at 40×24, resized to 20×8 without sending a key, confirmed a full-width divider and the draft at the new geometry, resized back to 40×24, then observed no output for 0.5 seconds with unchanged geometry. Log: `/tmp/gc-inline-independent-review.log`.
- Independent PTY checks at 65×30, 40×24, 24×16, 20×12 and 20×8 confirmed the pinned editor cursor position survives inline-reader redraw, bracketed paste and palette cancellation. Transcript/surface unit tests and scoped Clippy also pass; see the inline-scroll review.
- The same offline PTY matrix passed against the private release install at `/tmp/gc-final-install/bin/gc`; the existing default install was not replaced.

## Blocked checks

- The sandbox prohibits local provider listeners and the connection attempt is denied, so direct greeting-response behavior and continued live Bash execution cannot be verified against a responding mock or real provider.
- A direct idle resize signal was not emitted by the sandbox's standalone crossterm PTY probe. The app's 120 ms geometry polling fallback is now independently verified without relying on a resize signal.

No claim is made that a real model will answer a greeting directly. The prompt instructions support that behavior, but provider-level semantics remain untested here.
