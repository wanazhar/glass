# Native XMLHttpRequest upload lifecycle (418)

status: complete
scope: native-engine/xhr-upload-lifecycle
issue: 40
depends-on: [native-engine-browser-417, native-engine-browser-105]

## Objective

Make the native XMLHttpRequest upload surface observable for the buffered
request bodies that Glass already accepts. Page and worker XHR must expose a
real upload event target, report the exact byte length of text, URL-encoded,
Blob/File, FormData, and buffer bodies, and finish the upload lifecycle in the
same serialized turn as the request outcome.

## Contract

- Page and worker `XMLHttpRequest` instances expose an `upload` target with
  `addEventListener`, `removeEventListener`, and the standard upload handler
  properties for `loadstart`, `progress`, `load`, `error`, `timeout`,
  `abort`, and `loadend`.
- Accepted buffered bodies dispatch one upload `loadstart`, one byte-accurate
  `progress` record with `lengthComputable: true`, then exactly one terminal
  event and one `loadend`. The event target is the upload target, not the XHR.
- UTF-8 text, URLSearchParams, raw Blob/File, ArrayBuffer/view, and serialized
  FormData lengths use the same bytes that the existing request owner sends;
  binary bytes are never measured through lossy text conversion.
- XHR event listeners are retained alongside existing `on...` properties, and
  abort, timeout, error, and successful completion do not publish duplicate
  terminal events after stale continuations are suppressed.
- Unknown-length worker stream bodies may report a non-computable start and
  terminal lifecycle; this slice makes no false byte-total claim for them.
- This is a bounded buffered handoff lifecycle. It does not claim socket-level
  chunk progress, upload streaming backpressure, synchronous XHR, or full
  ProgressEvent/Web IDL parity.

## Implementation path

- Add shared page and worker upload event-target dispatch helpers beside the
  existing XHR bridges.
- Calculate body length from the already normalized request representation,
  preserving the current FormData boundary and byte limit.
- Route XHR and upload success/failure/abort/timeout completion through one
  terminal owner and retain stale-continuation guards.
- Add process-backed HTTP witnesses for page and worker XHR, including binary
  body bytes, upload event target identity, listener/handler delivery, and
  one-terminal-event ordering.
- Synchronize the architecture, plan, analysis, task record, and issue #40.

## Tradeoffs

- The progress record describes bounded body admission/completion, not a
  fabricated per-packet network trace; exact transport progress requires a
  streaming request-body owner and is left open.
- Reusing the existing serialization helpers keeps the reported total aligned
  with the bytes sent, at the cost of a bounded preflight measurement pass for
  FormData.
- A small native upload constructor improves event-target identity without
  exposing a second networking implementation.

## Evidence

- `cargo fmt --all` — passed.
- `cargo check --quiet -p glass-browser --tests --locked` — passed.
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_upload_reports_buffered_progress --locked -- --nocapture` — passed; a binary request body was observed byte-for-byte and the page upload target delivered one start, progress, load, and loadend sequence.
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_worker_xhr_upload_reports_buffered_progress --locked -- --nocapture` — passed; the worker upload target preserved identity and the same byte-accurate sequence.
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --nocapture` — passed; 7 existing XHR regressions remained green.
- `git diff --check` — passed.
- Documentation coverage, depth, and release-truth checks — passed.

Remote CI, push, release, tag, and registry publication are outside this
local checkpoint.
