---
id: native-engine-browser-141
scope: glass-browser/native-engine/request-lifecycle
status: completed
depends-on: [native-engine-browser-140]
---

# Native engine browser slice 141: request lifecycle and network quiet

Issue: [#40](https://github.com/wanazhar/glass/issues/40)

## Objective

Give the native runtime a parent-owned request-activity contract so ordinary
Glass synchronization can wait for a bounded quiet interval without observing
or creating a Chromium/CDP session.

## Contract

- native navigation loads, direct `fetch` operations, and external
  content-process evaluations bracket one bounded request operation;
- the parent ledger tracks bounded in-flight work, a monotonic completion
  sequence, and the most recent activity instant;
- the in-flight count is capped at 64 and fails closed if the bound is
  exceeded;
- native `network-quiet=<milliseconds>` waits use the ledger through
  `BrowserRuntimeSession`, CLI, and MCP native routing;
- successful wait state is diagnostic-safe and reports only in-flight count,
  quiet duration, required duration, and completion count;
- the operation remains serialized by the existing native session lock, so the
  ledger is an accounting boundary rather than an unbounded concurrent
  scheduler;
- no CDP fallback, hidden browser process, or third crate is introduced.

## Tradeoffs

This slice makes the normal quiet-wait contract usable immediately while
keeping the implementation aligned with the current content-process seam.
The ledger observes whole bounded native load/fetch/script operations, not
individual subresources or transport events inside them. That avoids inventing
false precision and avoids a new cross-process event protocol, but it leaves
per-resource lifecycle, redirect-chain, service-worker, streaming, and
transport-cancellation parity for later browser-complete slices.

The ledger is intentionally finite and in-memory. It is suitable for bounded
synchronization and diagnostics, not durable network history. Activity updates
are monotonic and the completion sequence is saturating so hostile or long
running pages cannot wrap accounting state.

## Implementation surface

- `browser/native_engine/engine.rs`: bounded ledger ownership around native
  navigation, fetch, and external script operations;
- `browser/native_backend.rs`, `browser/runtime.rs`: native quiet inspection
  and `WaitCondition::NetworkQuiet` routing;
- `tests/native_engine.rs`: external HTTP load and native-only quiet-wait
  regression coverage;
- `docs/plan/README.md`, `docs/architecture/native-engine.md`, and this task:
  synchronized scope, tradeoffs, and current-claim records.

## Verification

```text
cargo fmt --all
cargo check --quiet -p glass-browser --features native-engine --tests
cargo test --quiet -p glass-browser --features native-engine --test native_engine native_network_quiet_wait_reports_completed_request_activity -- --exact --nocapture
```

Completed evidence:

- formatting completed successfully;
- the feature-gated `glass-browser` test-target check passed;
- the local HTTP content-process test passed and confirmed
  `inFlight=0` with one completed native request operation after the quiet
  interval;
- no Chromium endpoint, CDP fallback, or third crate was introduced.

Per-resource lifecycle events, transport cancellation, service workers,
popup/download topology, continuation-aware prompts, universal workflow
parity, and native production promotion remain issue #40 work.
