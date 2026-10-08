---
id: composer-greeting-001
scope: submitted composer ownership and conversational tool intent
status: in-progress
depends-on: [live-turn-ui-001]
---

## User report

The user's phone screenshot shows the submitted `hello` in conversation and still in the composer while the model executes workspace Bash commands.

## Contract

- A successfully dispatched prompt clears and repaints the owned composer before any waiting, reasoning or tool progress. Provider/auth preflight failures retain the unsent prompt for correction. Typed follow-up drafts during work remain intact through viewer toggles, resize and completion.
- Queue and steering submission also clear their own visual input immediately, without deleting later draft input or duplicating session messages.
- Restore the divider to the full available terminal width with a one-cell safety margin. Its width follows resize, remains a single owned row, uses the existing border color, and never enters shell history. The previous eight-cell primary-screen workaround is obsolete with application-owned alternate rendering.
- The default system instructions distinguish conversation from actionable tasks: greetings and questions answer directly when no workspace evidence is needed; tools serve the user's actual request. Concrete coding requests retain investigation, implementation and verification behavior. No keyword-based routing, canned greeting interception, automatic workspace inspection or blanket tool prohibition.
- Preserve private session records, provider/auth configuration, headless operation and the existing rendering ownership model.

## Verification

Focused Rust contracts and real PTY submission/draft checks; independent review. Repeat provider-backed greeting and coding-task cases when localhost mock servers are permitted. Current environment denies localhost listeners and default installation outside repo/tmp; report blocked checks explicitly. Build a private release only after fixes and feasible verification.
