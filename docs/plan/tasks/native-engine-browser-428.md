# Native page XHR streaming (428)

status: complete
scope: native-engine/page-xhr-streaming
issue: 40
depends-on: [native-engine-browser-427]

## Objective

Advance page XMLHttpRequest from buffered body completion to demand-driven
response-body consumption. Applications should observe `LOADING` and response
`ProgressEvent` records as the existing native Fetch response stream delivers
bounded chunks, while terminal response decoding remains unchanged.

## Contract

- Page XHR consumes the existing native response `ReadableStream` reader and
  publishes one `LOADING`/`progress` pair for each admitted response chunk.
- Text response projections preserve UTF-8 text across chunk boundaries;
  binary, JSON, Blob, and document response types keep their existing final
  response projections and do not expose `responseText`.
- Abort cancels the active response reader and suppresses late chunk and
  terminal callbacks; successful completion still emits one `DONE`/`load`/
  `loadend` sequence.
- Empty-body and buffered/synthetic responses remain valid through the same
  finalization path.
- Worker XHR remains buffered in this slice because worker Fetch currently
  resolves through a separate whole-response host path; worker streaming is a
  follow-up contract, not an implicit claim here.

## Tradeoffs

- Reusing the existing Fetch stream owner avoids a second transport and keeps
  backpressure/cancellation centralized, but page XHR now schedules more
  realm turns for chunked responses.
- Incremental text decoding retains a bounded raw-byte accumulator for final
  binary/JSON/document conversion; the existing response-body limit remains
  authoritative.
- The implementation is stream-progress parity for page XHR, not synchronous
  XHR, streaming upload, response trailers, or complete XHR/Streams Web IDL
  parity.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_stream --locked -- --nocapture` (2 passed)
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --nocapture` (16 passed)
- `git diff --check`

The focused HTTP witness observed multiple page response chunks and passed;
the complete XHR regression group also passed. Remote CI, push, release, tag,
and registry publication are outside this local checkpoint.
