---
id: native-engine-browser-154
scope: glass-browser/native-engine/navigation-lifecycle-reentry
status: completed
depends-on: [native-engine-browser-153]
---

# Native engine browser slice 154: lifecycle navigation re-entry

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Complete the parent-owned navigation handoff for page lifecycle and
same-document event callbacks. A page that changes `location` from
`beforeunload`, `pagehide`, `unload`, `popstate`, or `hashchange` must reach the
same bounded loader/history/frame owner as a direct Glass navigation in both
local and sandboxed HTTP(S) realms.

## Contract

- outgoing local and content-process `beforeunload` callbacks may return one
  validated location navigation after their normal cancel decision;
- outgoing `pagehide`/`unload` callbacks may return one validated location
  navigation, while their lifecycle effects remain ordered before the next
  committed document;
- local and content-process `popstate`/`hashchange` callbacks may return one
  validated location navigation after the same-document URL/history mutation;
- a lifecycle-produced navigation skips only the lifecycle dispatch that
  already produced it, then uses the ordinary bounded navigation owner;
- cancellation remains distinct from a completed lifecycle with no handoff;
- local and HTTP(S) paths retain history replacement/push behavior, storage
  publication, frame policy, target ownership, and the eight-handoff limit;
- a second navigation emitted by one lifecycle boundary fails closed rather
  than choosing an order implicitly.

## Tradeoffs

The handoff is synchronous with the owning lifecycle boundary, so the parent
does not expose a half-committed document or a second navigation queue. The
already-dispatched outgoing lifecycle is skipped exactly once for its own
handoff; a newly committed document still receives its normal lifecycle. This
keeps recursion bounded and preserves event ordering, at the cost of rejecting
ambiguous multiple lifecycle navigations instead of guessing browser-specific
queue behavior.

## Implementation surface

- `browser/native_engine/engine.rs`: local/content lifecycle return values,
  same-document callback handoffs, bounded navigation-owner recursion, and
  cancellation distinction;
- `tests/native_engine.rs`: local and HTTP(S) witnesses for outgoing lifecycle
  replacement and hashchange re-entry;
- `docs/plan/README.md`, `docs/architecture/native-engine.md`, and
  `docs/plan/analysis/native-engine.md`: current issue status and remaining
  browser-parity gates.

## Verification

```text
cargo fmt --all -- --check
git diff --check
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine lifecycle_can_replace_navigation -- --nocapture
cargo test --quiet -p glass-browser --features native-engine --test native_engine can_reenter_navigation -- --nocapture
cargo test --quiet -p glass-browser --features native-engine --test native_engine -- --nocapture
```

The focused witnesses pass 4/4, and the full native integration target passes
with 436 tests. Static documentation/release-claim validation is recorded
with the checkpoint. Strict affected-package lint, workspace/release gates,
remote CI, and native default promotion remain issue #40 closure gates.
