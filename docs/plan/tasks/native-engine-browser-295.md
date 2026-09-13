# Glass native engine browser slice 295: worker abort signals

Status: completed locally.

## Objective

Make worker Fetch observe the same abort contract as the page Fetch surface,
while keeping resource policy and transport ownership in the shared native
loader.

## Contract

- Dedicated workers expose stable `AbortController` and `AbortSignal`
  constructors with `aborted`, `reason`, `onabort`, abort listeners, and
  `throwIfAborted()`.
- `AbortController.abort()` is idempotent, preserves an explicit reason, and
  dispatches one bounded abort event to listeners and `onabort`.
- `AbortSignal.abort()`, bounded `AbortSignal.timeout()`, and bounded
  `AbortSignal.any()` are available; composed signals preserve the first
  abort reason and clean up their source listeners.
- Worker `Request` validates and retains a signal. Worker Fetch rejects an
  already-aborted request, registers a request-local abort listener, removes
  it on settlement, and rejects/removes pending state when abort occurs.
- A host result arriving after worker abort is ignored because the pending
  request was removed before rejection; no response can be delivered to the
  wrong worker promise.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

The worker bootstrap now installs re-entrant abort-signal/controller
constructors, event dispatch, static composition helpers, and bounded timer
integration for timeout signals. The worker Fetch map stores its signal and
abort listener with each pending request. Aborting removes the map entry,
detaches the listener, and rejects with the supplied reason before the shared
loader can resolve the request. The existing owner-tagged command and loader
policy remain unchanged, and Worker Request objects receive a fresh signal by
default.

The local witness covers constructor identity, explicit abort reasons,
listener delivery, `AbortSignal.any()`, default Request signal state, and
pre-aborted Fetch rejection. It also asserts that no worker error event is
generated.

## Tradeoffs and follow-up

Request-local cancellation gives correct worker-observable promise behavior
and prevents late-result delivery without duplicating or weakening the shared
loader. An already-running host request is still allowed to finish in the
current slice; the worker simply ignores its result. Actual transport
cancellation, timeout-signal cleanup, complete abort-event/Web IDL
semantics, automatic background task scheduling, and the final native/CDP
replacement gates remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_worker_fetch_honors_abort_signal --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine worker --locked` (19 passed)
- `git diff --check`

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider Issue #40 gates.
