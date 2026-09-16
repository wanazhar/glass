# Native page XHR reopen cancellation (429)

status: complete
scope: native-engine/page-xhr-reopen
issue: 40
depends-on: [native-engine-browser-428]

## Objective

Close the lifecycle hole exposed by page XHR streaming: reopening an XHR
while a response reader is active must cancel the old reader and prevent its
late continuation from publishing state, progress, or terminal callbacks on
the reused object.

## Contract

- `open()` cancels any active response reader before replacing request state.
- A stale stream continuation cannot mutate the reopened XHR or dispatch
  response events after ownership has moved to the new request.
- Existing explicit `abort()` cancellation, successful streaming completion,
  response decoding, and event ordering remain unchanged.

## Tradeoffs

- Cancellation is local to the existing response reader and request identity;
  no new transport or process boundary is introduced.
- The old request may already have delivered an event before `open()` runs;
  only later continuation work is suppressed.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_reopen_cancels_stale_response_reader --locked -- --nocapture`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_streams_page_response_progress --locked -- --nocapture`
- `git diff --check`

The reopen-during-stream witness and the existing chunked-stream witness both
passed. Remote CI, push, release, tag, and registry publication are outside
this local checkpoint.
