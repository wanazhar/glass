# Native Glass live `navigate-to` policy transfer (412)

status: done
scope: native-engine/csp-navigate-to-dynamic-transfer
issue: 40

## Objective

Make a CSP `navigate-to` policy inserted or changed during a live content
turn authoritative for the parent-owned top-level navigation decision. The
next link, script/location, download, popup, history, or other top-level
handoff must observe the updated policy without requiring a reload.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-411.md`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- [W3C Content Security Policy Level 3](https://www.w3.org/TR/CSP3/)

Slice 411 transferred response and initial head-meta source groups, but its
parent snapshot stayed unchanged after a live DOM mutation. The content
process already owns the append-only CSP policy container and applies newly
inserted meta policies before resource and navigation effects; the missing
piece was a typed transfer back to the parent navigation owner.

## Contract

- Every content-process response that carries a mutated document snapshot
  includes the current bounded effective `navigate-to` source groups. The
  existing loaded-document envelope remains compatible with the same field.
- The child recomputes the groups after the complete turn, including script,
  input, lifecycle, hash-change, and event-loop mutations. Multiple enforced
  declarations remain intersected by the shared loader; removing or editing a
  processed meta element cannot relax the append-only policy container.
- The parent decodes the field with the existing source-group limits and
  validation, then commits it before the lifecycle-only fast path. A live
  policy therefore applies even when the document snapshot is otherwise
  unchanged.
- The content-worker protocol version advances with the wire contract. No
  unbounded raw CSP headers or policy text cross the process boundary.
- A missing explicit `navigate-to` directive remains `None`; it does not
  inherit `default-src`, and a new document can clear the prior document's
  snapshot.

## Non-goals

This slice does not define `navigate-to` as a normative CSP Level 3 directive;
Glass documents it as an owned navigation-policy extension. It does not claim
report-only delivery through every navigation path, add new navigation APIs,
change redirects or form-action semantics, or complete the remaining CSP and
Core Web Profile conformance gates in issue #40.

## Implementation path

- Add the bounded source-group field to the existing decoded content mutation
  envelope and reuse the loader's navigation-policy decoder.
- Inject the current child policy once at the shared response boundary rather
  than duplicating it in each input/mutation branch.
- Store the validated snapshot in the parent before checking whether the
  mutation is lifecycle-only, preserving the current parent ownership model.
- Add an HTTP content-process witness that inserts a CSP meta policy after
  load, clicks a link, and proves that no second request is issued.

## Tradeoffs

- Repeating a small bounded source-group projection on mutation responses adds
  IPC bytes, but avoids exposing response headers and keeps the parent
  independent of the child's private policy-container ledger.
- Central response-boundary projection keeps all mutation paths covered and
  makes future content-turn additions inherit the policy contract; it means
  the child performs one policy lookup even for mutations that do not navigate.
- The parent remains authoritative for effects it owns. The child can reject
  its own resource work, but cannot be the sole guard for target creation,
  history, downloads, popups, or final document commits.

## Delivered

- Bumped the content-worker wire version from 11 to 12 for the mutation policy
  field.
- Added bounded `navigate_to_sources` decoding to content mutations and
  central child-side projection for mutated/evaluated document envelopes.
- Committed the validated snapshot in the parent before lifecycle-only
  short-circuiting.
- Added a regression witness for post-load meta-policy insertion blocking a
  parent-owned link navigation without a follow-up HTTP request.
- Synchronized architecture, active plan, and analysis records.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_propagates_dynamic_navigate_to_to_parent_navigation_owner --locked -- --exact --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_navigate_to_ --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_form_action_blocks_click_and_script_submit --locked -- --exact --nocapture` (1 passed)

Remote CI, push, release, tag, and registry publication remain outside this
local checkpoint.
