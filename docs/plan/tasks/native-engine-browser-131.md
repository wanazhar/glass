---
id: native-engine-browser-131
scope: glass-browser/native-engine/public-session-revisions
status: completed
depends-on: [native-engine-browser-130]
---

# BE-45: native public-session revision ownership

## Objective

Bring the native semantic session's mutation contract in line with the normal
Glass session: callers must be able to bind a navigation or action to the
revision they observed, and concurrent calls on one session must not turn a
check-then-mutate sequence into a stale write.

## Contract

- `BrowserRuntimeSession` exposes revision-aware navigation and semantic action
  methods.
- A guarded call compares the current evidence revision and rejects a stale
  expectation before the mutation is dispatched.
- The compare-and-dispatch sequence is serialized per runtime session. The
  native engine remains the single owner of document revision increments.
- Native CLI commands use the existing `--expected-revision` fields for
  navigate, click, type, clear, check, uncheck, select, key, key-down, key-up,
  and shortcut instead of rejecting them as an alternative-runtime-only gap.
- Unchecked calls preserve the existing portable semantic API. No native call
  may create a CDP connection or silently switch to another backend.

## Deliberate boundary and tradeoffs

The guard is implemented at the portable session seam rather than by adding
revision fields to every wire request. This preserves the versioned backend
protocol and keeps the native engine's revision transaction authoritative. A
session-local async mutex makes the read/dispatch pair atomic for callers that
share one `BrowserRuntimeSession`; independent sessions remain independent
browser contexts. Cross-process multi-client leases and richer action outcome
metadata remain later integration work.

## Paths

- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/cli/runner.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

- scoped native-feature library check;
- focused native runtime-session revision tests;
- focused native CLI validation tests;
- formatting, diff, documentation coverage/depth, and release-truth checks.

This slice does not promote native certification or claim CDP replacement. It
removes a public-session correctness gap needed before default routing can be
considered.
