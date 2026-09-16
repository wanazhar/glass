# Native form-action report delivery (414)

status: complete
scope: native-engine/csp-form-action-report-delivery
issue: 40
depends-on: [native-engine-browser-410, native-engine-browser-413]

## Objective

Deliver report-only `form-action` CSP violations through the owning HTTP(S)
content page before a form submission is allowed or rejected. Cover native
click submission and page-script submission while preserving the child-owned
form-policy decision, submit-event ordering, and the parent navigation owner.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-410.md`
- `docs/plan/tasks/native-engine-browser-413.md`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- [W3C Content Security Policy Level 3](https://www.w3.org/TR/CSP3/)

Slice 410 made the explicit `form-action` policy decision shared and
report-aware, but the child loader's queued report was not drained by the
click or script-submit mutation turn. Slice 413 completed the separate
parent-owned `navigate-to` preflight. The `form-action` vocabulary remains a
Glass-owned policy extension in this repository; this task does not present
it as a normative CSP Level 3 directive.

## Contract

- The content process remains the sole owner of the current HTTP(S) document's
  `form-action` policy container and report queue.
- After submit event handlers have settled and the final form target is known,
  the child evaluates enforced and report-only `form-action` together. The
  boolean result controls whether a form navigation record is emitted.
- Queued report-only violations travel with the mutation envelope and are
  dispatched through the existing persistent page
  `SecurityPolicyViolationEvent` bridge after the parent commits the child
  snapshot, before the parent-owned form navigation proceeds. Report-only
  records do not change authorization.
- Click submission and `HTMLFormElement.submit()`/requestSubmit-style script
  submission share the same drain. A blocked form still produces its report
  without a network request; an allowed form can navigate after the report
  listener turn completes.
- A report listener's bounded DOM/history/storage/dialog/navigation effects
  retain existing event-queue limits and conflict rules. No second policy
  matcher or CDP fallback is introduced.

## Non-goals

This slice does not broaden `form-action` grammar, change submit validation or
history semantics, add report delivery for unrelated mutation resources, or
claim complete CSP/Core Web Profile certification.

## Path

- Transfer pending CSP records after the child form-action check in the click
  submit mutation path, then deliver them after the parent commits the
  mutation and before navigation.
- Drain the same records in the page-script mutation path, preserving the
  existing child-side event-turn ordering and single-navigation rule.
- Add HTTP witnesses for blocked and report-only-allowed submissions and
  assert event ordering, request behavior, and page state.

## Tradeoffs

- The child remains the policy owner, while the parent performs delivery after
  committing the child snapshot; this adds one bounded page-event turn only
  when records exist and prevents listener DOM effects from being overwritten
  by the mutation transfer.
- A report listener runs before navigation continues, so its work contributes
  to submit latency and retains the existing bounded event-turn failure
  behavior.

## Evidence

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine form_action --locked -- --nocapture` (6 passed)
- `cargo test --quiet -p glass-browser --test native_engine navigate_to --locked -- --nocapture` (3 passed)
- `cargo fmt --all`

The blocked click and page-script submission witness each deliver one
report-only `form-action` event without issuing `/result`; the allowed
report-only witness delivers its event and then issues `/result?query=hello`.
Remote CI, push, release, tag, and registry publication remain outside this
local checkpoint.
