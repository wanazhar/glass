# Native engine browser-complete slice 431: inline worker response streams

status: complete
scope: native-engine/inline-worker-response-streaming
issue: 40
depends-on: [native-engine-browser-430]

## Objective

Close the remaining inline-owner worker response-streaming gap for the
deterministic fixture profile. Worker Fetch in the in-process `NativeEngine`
must use the same bounded demand-driven stream ownership as the
content-process HTTP(S) path instead of silently resolving a whole response
buffer.

## Contract

- `NativeEngine` constructs its worker registry with response streaming
  enabled.
- Registered fixture owners may issue bodyless same-fixture-host worker Fetch
  requests and receive a bounded stream response.
- Fixture response bytes are split into the shared 8 KiB transport parts and
  are admitted only after worker reader demand.
- The inline engine pumps worker stream events at serialized page boundaries,
  allowing worker readers, clones, and cancellation to use the existing
  ownership checks.
- Fixture request methods remain fail-closed to bodyless `GET`/`HEAD`; remote
  HTTP(S) request policy continues to be owned by the content process.

## Tradeoffs

- Reusing the shared stream connection keeps limits, cancellation, clone
  queues, and worker command validation identical across owners, at the cost
  of a bounded scheduler turn at each inline page boundary.
- The fixture transport is a deterministic byte source, not a second network
  stack. It proves inline ownership and chunk behavior without weakening the
  HTTP(S) content-process security boundary.
- Page-owned inline Fetch, streaming request uploads, synchronous XHR, and
  complete XHR/Streams Web IDL parity remain separate issue #40 work.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_local_worker_fetch_streams_fixture_response_body --locked -- --nocapture`
- `cargo test --quiet -p glass-browser --test native_engine native_local_worker_ --locked -- --nocapture` (10 passed)
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_worker_fetch_ --locked -- --nocapture` (3 passed)
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_worker_xhr_ --locked -- --nocapture` (2 passed)
- `git diff --check`

The inline witness admitted two chunks (`8192` and `7` bytes), preserved a
four-byte UTF-8 character split across the transport boundary, retained the
expected response URL/status, and verified independent clone/original body
ownership. The broader worker WebSocket name filter was not used as slice
evidence because an unrelated server close-frame race failed once; its
Fetch/XHR subsets passed independently.
