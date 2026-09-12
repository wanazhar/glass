# Glass native engine browser slice 250: async fetch evaluation

Status: completed locally.

## Objective

Make top-level JavaScript `await` work across the native content-process
boundary when the awaited operation is a native fetch. Page code must be able
to use the normal `await fetch(url)` and `await response.text()` form without
deadlocking the QuickJS worker or falling back to the CDP backend.

## Contract

- Already-settled top-level-await expressions retain their existing result and
  exception behavior.
- When top-level evaluation queues a native fetch, QuickJS returns control to
  the content worker with the fetch command still visible; the worker owns the
  network request and resumes the JavaScript continuation through the existing
  `__glassResolveFetch` path.
- The final fulfilled value is returned through `NativeEngine::evaluate_async`,
  including response-body continuations such as `await response.text()`.
- A rejected top-level promise becomes a bounded native script error after the
  host has serviced the pending fetch; no unresolved promise or fetch command
  is reported as successful.
- Promise state remains inside the JavaScript realm. No rquickjs `Persistent`
  handle crosses the backend's `Send + Sync` boundary.

## Implementation

The evaluator now attaches bounded fulfillment/rejection callbacks to the
QuickJS top-level evaluation promise. If QuickJS reports `WouldBlock`, the
evaluator returns an undefined placeholder, marks the evaluation pending, and
leaves the fetch command in the normal script-command stream. The content
worker passes that marker through `resolve_script_fetches`; after each response
it drains pending jobs, checks the realm-owned state, and replaces the
placeholder with the serialized fulfilled value. The same resolver path keeps
processing chained fetches and body-reading promises.

The result bridge reads a small JSON state record from the realm, enforces the
existing script-result limit, unwraps QuickJS's top-level `{ value: ... }`
evaluation envelope, clears settled state, and converts rejection text into a
typed worker error. The test fixture now exercises every common method using
top-level `await fetch` and `await response.text` rather than a separate
promise callback workaround.

## Tradeoffs and follow-up

The realm-owned state marker is intentionally narrow: it solves host-backed
fetch continuations without making the browser backend carry a non-thread-safe
QuickJS persistent handle. A top-level promise that waits only on a timer or
another future without a host command can remain pending until the existing
event-loop owner advances it; full task scheduling, worker execution, and
streaming/event-source integration remain later browser-completeness work.
The evaluator still permits only one bounded native evaluation transaction at
a time, preserving document and mutation ownership.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_fetches_common_http_methods_with_cors_preflight --locked` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_evaluates_persistent_script_realm --locked` (1 passed)
- `git diff --check`

The evidence is local-only. Remote CI, push, release, registry publication,
and final native-engine parity claims remain unclaimed for this checkpoint.
