# Native engine browser-complete slice 443: XHR load-start lifecycle

status: complete
scope: native-engine/xhr-loadstart
issue: 40
depends-on: [native-engine-browser-442]

## Objective

Complete the first XHR progress-event lifecycle boundary. Every admitted
native XHR request must notify the XHR target with a bounded `loadstart`
`ProgressEvent` before upload progress and before the response owner begins,
including synchronous requests.

## Contract

- Page and dedicated/SharedWorker XHR dispatch one XHR-target `loadstart`
  `ProgressEvent` at request start with the XHR as both target and current
  target, `lengthComputable: false`, and zero loaded/total bytes.
- The XHR-target event precedes the upload-target `loadstart` event; existing
  upload progress and terminal records remain unchanged.
- The event is emitted for async and synchronous XHR after request validation
  admits the body and before transport/host work can complete.
- EventTarget listener identity, handler delivery, exception isolation, and
  existing bounded response/cancellation/lifecycle ownership remain intact.

## Tradeoffs

- The event is a lifecycle boundary, not socket-level progress; it deliberately
  reports zero bytes and does not claim that the transport has begun reading
  from a peer.
- Requests rejected before body admission do not emit `loadstart`, avoiding a
  false successful-start signal. Existing bounded upload notifications retain
  their body-size knowledge and ordering.
- Synchronous XHR still blocks the caller after the event is delivered; no
  asynchronous pump or second transport owner is introduced.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_worker_xhr_upload_reports_buffered_progress --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_upload_reports_buffered_progress --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --test-threads=1 --nocapture` (21 passed, 0 failed)
- `git diff --check`

The page and worker upload witnesses assert the XHR-target `ProgressEvent`
shape and target identity in addition to their existing upload progress and
response assertions.
