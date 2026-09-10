---
id: native-engine-browser-096
scope: glass-browser/native-engine/xhr-abort
status: done
depends-on: [native-engine-browser-095]
---

# BE-17: bounded XHR abort semantics

## Objective

Give asynchronous native `XMLHttpRequest` callers an observable abort path by
reusing the existing bounded page `AbortController` implementation.

## Contract

- `XMLHttpRequest.abort()` cancels the observable request state and resets an
  active request to `UNSENT` with cleared response metadata.
- An active abort dispatches one `readystatechange` transition and one
  `abort` callback; late `load` and `error` callbacks cannot mutate the XHR.
- A request-specific internal abort signal is detached from the fetch bridge
  when the request settles or is aborted.
- Existing asynchronous GET/POST methods, request/response header limits,
  Blob/File bodies, response bodies, credentials, CORS, redirects, and size
  policies remain unchanged.
- Socket-level cancellation, timeout, progress/upload events, binary response
  types, synchronous XHR, complete ready-state/event ordering, and full
  XHR/Fetch Web IDL parity remain open.

## Ownership and sequence

```text
XHR.send() -> request-local AbortController -> fetch bridge
XHR.abort() -> signal rejection -> stale continuation guard -> UNSENT/abort event
```

The JavaScript XHR wrapper owns observable state and event delivery. The
existing fetch bridge owns the bounded request command and may finish an
already-issued host operation; no stale host result can re-enter the XHR.

## Deliberate boundary and tradeoffs

Using the existing signal path avoids a second cancellation protocol and keeps
fetch/XHR rejection behavior aligned. The tradeoff is resource cancellation
is still observable rather than transport-level: the host request can consume
its existing bounded deadline and response budget after page code aborts.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

Evidence is recorded after the affected compile, XHR abort behavior, full
native-engine target, documentation, and release-truth gates complete. The
checkout is local-only: no push, remote CI, release, tag, or registry-
publication claim is made by this task.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_xhr_abort_is_observable_and_ignores_late_callbacks -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine` — 382 passed, 0 failed
- `cargo clippy --quiet -p glass-browser --features native-engine --test native_engine --bin glass-native-content-worker -- -D warnings` — expected non-zero baseline of exactly 32 documented pre-existing diagnostics; normalized diagnostics match the 094 baseline and no new diagnostic was introduced
- `python3 scripts/check-documentation-coverage.py` — 746 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  746 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=919; current-claim failures=0
