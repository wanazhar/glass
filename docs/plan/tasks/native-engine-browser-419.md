# Native page XMLHttpRequest response lifecycle (419)

status: complete
scope: native-engine/xhr-response-lifecycle
issue: 40
depends-on: [native-engine-browser-418]

## Objective

Bring the page XMLHttpRequest response state machine in line with the worker
bridge and the declared Glass XHR contract. A successful asynchronous response
must publish headers-received and loading transitions before completion, while
abort, timeout, errors, and stale continuations retain their existing terminal
ownership.

## Contract

- A page XHR response observes `OPENED` -> `HEADERS_RECEIVED` -> `LOADING` ->
  `DONE` (`1 -> 2 -> 3 -> 4`) in order for a successful asynchronous request.
- `status`, `statusText`, `responseURL`, response headers, and content type are
  available by `HEADERS_RECEIVED`; the final body value is available by
  `DONE`.
- Each state transition uses the existing XHR event-listener and
  `onreadystatechange` dispatch owner, and stale/aborted continuations cannot
  publish later states or terminal events.
- Response body delivery remains bounded and buffered as today. This slice
  does not claim incremental network chunks, synchronous XHR, responseXML, or
  complete XHR/Web IDL parity.

## Implementation path

- Publish `HEADERS_RECEIVED` immediately after the native response metadata is
  admitted and before body consumption.
- Publish one bounded `LOADING` transition after the response body consumer
  has been admitted and before `DONE` publication.
- Add a process-backed page XHR state-trace witness and retain the existing
  page/worker abort, timeout, upload, and response regressions.
- Synchronize the architecture, plan, analysis, task record, and issue #40.

## Tradeoffs

- The native loader currently returns a bounded buffered response value, so the
  loading transition is a truthful lifecycle boundary rather than fabricated
  per-chunk progress.
- Reusing the existing callback dispatcher keeps page and worker exception
  isolation consistent without adding another event queue.

## Evidence

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_publishes_response_state_lifecycle --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --nocapture` (8 passed)
- `cargo fmt --all`
- `git diff --check`
- documentation coverage, depth, and release-truth checks passed

Remote CI, push, release, tag, and registry publication are outside this
local checkpoint.
