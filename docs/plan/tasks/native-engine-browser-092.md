---
id: native-engine-browser-092
scope: glass-browser/native-engine/fetch-abort
status: done
depends-on: [native-engine-browser-091]
---

# BE-13: bounded fetch AbortController semantics

## Objective

Provide the native fetch callers with a usable bounded
`AbortController`/`AbortSignal` surface. A caller must be able to observe
abort state, reason, and one abort event, and an associated fetch promise must
reject instead of later resolving after the caller aborts it.

## Contract

- `AbortController` exposes a stable `signal`; `abort(reason)` is idempotent,
  records the first reason, sets `aborted`, dispatches one `abort` event, and
  invokes `onabort` once.
- `AbortSignal` supports bounded `addEventListener`,
  `removeEventListener`, and `throwIfAborted` behavior for the native signal.
- `fetch()` validates a supplied signal, rejects immediately for an already
  aborted signal, and rejects an observable pending request with the signal's
  reason when it aborts.
- Resolved/rejected fetch entries detach their abort listener, and late host
  responses for an aborted request are ignored.
- Existing request, response, origin, CORS, credentials, redirect, method,
  and size limits remain authoritative. The content-worker fetch path is
  covered.
- Transport-level socket cancellation, XHR `abort()`, timeout/progress,
  `AbortSignal.timeout/any`, and complete DOM/Web IDL parity remain open.

## Ownership and sequence

```text
controller.abort() -> signal event/state -> pending fetch rejection
host response -> ignored when the observable request was aborted
```

The JavaScript realm owns signal state and promise settlement. Rust and the
content worker retain the existing bounded network request; an already-issued
host request may finish, but its response cannot re-settle the aborted page
promise.

## Deliberate boundary and tradeoffs

Removing the pending promise before host completion gives page code the
required observable cancellation quickly without adding a new IPC cancellation
frame or risking a request-ID race. The tradeoff is resource cancellation is
not yet real: the network operation still consumes its existing bounded
deadline and response budget. A later slice can add owner-level cancellation
once that protocol is explicit.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, fetch/abort behavior, full
native-engine target, documentation, and release-truth gates complete. The
checkout is local-only: no push, remote CI, release, tag, or registry-
publication claim is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 381 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; no new diagnostics
- `python3 scripts/check-documentation-coverage.py` — 742 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version`
  — 742 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=915; current-claim failures=0
