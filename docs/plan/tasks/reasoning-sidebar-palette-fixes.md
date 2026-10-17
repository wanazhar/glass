# Provider reasoning and mobile interaction fixes

Status: in-progress. User reported missing messages[2].reasoning_content after tool execution, indistinguishable project expansion, and first-focus keyboard palette scroll lock.

## Contract

- Preserve and correctly associate provider-returned reasoning needed for subsequent tool rounds and resumed requests. Use durable session data and bounded context; do not fabricate reasoning, expose encrypted/private provider payloads, resend tool effects, auto-retry billed requests or suppress provider errors. Keep provider formats isolated and preserve source records. Inspect official provider contracts and every projection path.
- Project headings expose an obvious disclosure chevron and distinct open state; expanded thread rows indent and group visually. Preserve native details keyboard semantics, current workspace names/counts, search/active-thread behavior, readability, colors and performance. No extra requests or heavy icons.
- Selecting a palette category by touch should not force the keyboard open unless text input was requested. Opening for typing must permit command-list touch scrolling on the first keyboard transition as well as subsequent ones. Preserve centered safe-area/visual-viewport sizing, command taps, keyboard navigation, user-initiated input focus and visible headers/footers. No global touchmove cancellation or artificial delays.

Layout:

    > folder  project                         count
      /project/path
    v folder  project                         count  [open surface]
      /project/path
        thread
        thread

## Delivery

Separate Rust provider/runtime and frontend tasks, independent review. No test suites added/run unless requested; builds, syntax and source review permitted. No live inference or user prompt replay. Native deployment can require a rebuild and runtime update; finish the concrete change before approval if interrupting active work. UI assets can deploy reversibly without runtime interruptions. Document actual evidence and remaining physical iPhone verification.

## Frontend delivery

- Merged `044ea34`: explicit native project disclosure, shaded open headings and grouped threads; touch category browsing without forced keyboard focus; stable palette rows and scoped scrolling during keyboard transitions.
- Independent source review passed. Frontend build and syntax checks passed; no test suites run. Static visual review covered sidebar states. Physical iPhone first-keyboard touch scrolling remains unverified.
- Reversible web asset deployment served shell `91d5d80cc2e7`; public build metadata matches installed files. No gateway or chat runtime restart.
- Provider reasoning implementation and independent review remain in progress.
