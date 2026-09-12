# Glass native engine browser slice 251: host event-loop timer turns

Status: completed locally.

## Objective

Drive pending top-level JavaScript evaluation through the native content
process when progress is owned by a page timer or animation frame. A timer
callback must be able to mutate the document, start a native fetch, and settle
the enclosing promise before Glass publishes the script result.

## Contract

- A pending top-level `await` with a due page timer waits for the bounded next
  timer deadline instead of being rejected as an unsupported host operation.
- The host executes one timer/frame turn at a time with the bootstrap timer
  pump disabled for that turn; QuickJS pending promise jobs are drained after
  the callback.
- Timer-originated DOM/history mutations use the existing content-process
  mutation and document snapshot owners.
- Timer-originated fetches use the existing native request/CORS/redirect
  owner, and their continuations can settle the original top-level await.
- The event-loop turn count and fetch/effect counts remain bounded by existing
  native limits, and the existing content-process script timeout remains the
  outer deadline.
- If no host operation can make a pending promise progress, the worker returns
  a typed error; it never reports an unresolved promise as a successful null
  result.

## Implementation

`NativeJavaScriptRuntime` now exposes the next bounded timer/frame delay and a
single-turn runner. The runner temporarily disables the bootstrap's automatic
timer invocation, evaluates the internal timer-turn source, and restores the
previous mode after QuickJS drains pending jobs.

The content-process script path now routes pending evaluations through one
bounded loop. It drains all queued fetches, checks the realm-owned promise
state, sleeps until the next due timer when needed, applies timer commands to
the current document, and returns any new fetch commands to the same loop.
This keeps timer callbacks, fetch continuations, document mutations, and
top-level-await settlement in one serialized content owner.

## Tradeoffs and follow-up

The loop uses the realm's monotonic clock and real Tokio sleep, so it makes
ordinary delayed script useful without adding a second scheduler or exposing
QuickJS handles across the backend's thread boundary. A bounded turn cap
prevents interval or self-rescheduling callbacks from monopolizing a content
request; the existing script timeout and effect limits provide additional
backstops. Animation frames share the same due-time inspection and turn path,
but rendering-frame cadence and compositor synchronization remain later work.

This slice does not implement Web Workers, Service Workers, WebSocket,
EventSource, background-page task persistence, streaming response delivery, or
full HTML/ECMAScript event-loop conformance. Those require their own lifecycle,
IPC, cancellation, and cross-context contracts before issue #40 can be closed
or the native engine can be promoted as the sole production browser backend.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_drives_timer_await_and_publishes_mutation --locked` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_drives_timer_started_fetch_continuation --locked` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_resolves_page_script_fetch_before_publish --locked` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_fetches_common_http_methods_with_cors_preflight --locked` (1 passed)
- `git diff --check`

The evidence is local-only. Remote CI, push, release, registry publication,
and final native-engine parity claims remain unclaimed for this checkpoint.
