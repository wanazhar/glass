# Native XHR download progress (427)

status: complete
scope: native-engine/xhr-download-progress
issue: 40
depends-on: [native-engine-browser-426]

## Objective

Complete the bounded buffered-download progress event for page and worker XHR.
Callers should observe one truthful response-body progress record before the
terminal callback, with byte counts derived from the admitted response body and
its validated `Content-Length` metadata when available.

## Contract

- Page and worker XHR dispatch one response `progress` `ProgressEvent` after
  bounded body buffering and while `readyState` is `LOADING`.
- The event preserves XHR target identity and reports loaded bytes; a valid
  non-negative bounded `Content-Length` makes `lengthComputable` true and is
  exposed as `total`, otherwise `lengthComputable` is false and `total` is zero.
- The event is delivered before `load`/`loadend` and does not create a second
  terminal sequence on abort, timeout, or error.
- Existing upload progress, response projections, `responseText` guards,
  response-type decoding, lifecycle ordering, and worker isolation remain
  unchanged.
- This is buffered observability only; socket-level chunk progress, streaming
  XHR, synchronous XHR, and complete ProgressEvent/XHR Web IDL parity remain
  separate issue #40 gates.

## Implementation path

- Derive bounded loaded/total byte counts in the existing page and worker XHR
  body-completion turns and dispatch the realm's existing `ProgressEvent`.
- Add a real HTTP witness for page and worker progress identity, target,
  byte-count fields, ready state, ordering, and missing-length behavior; retain
  the complete XHR regression group.
- Synchronize architecture, plan, analysis, task, and issue #40.

## Tradeoffs

- One event after buffering is deterministic and truthful for the current
  owner, but it does not give applications incremental network feedback.
- Response bytes are measured from the decoded response value where the loader
  does not expose raw bytes directly; explicit content length remains bounded
  and is never trusted outside the admitted range.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_reports_buffered_download_progress --locked -- --nocapture`
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --nocapture` (14 passed)
- `git diff --check`

All scoped checks and the complete XHR regression group passed locally. Remote
CI, push, release, tag, and registry publication are outside this local
checkpoint.

Remote CI, push, release, tag, and registry publication are outside this
local checkpoint.
