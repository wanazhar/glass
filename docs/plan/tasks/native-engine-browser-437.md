# Native engine browser-complete slice 437: fixture stream uploads

status: complete
scope: native-engine/fixture-fetch-request-upload-streaming
issue: 40
depends-on: [native-engine-browser-436]

## Objective

Close the inline fixture owner's Worker Fetch request-body streaming gate. A
Worker `ReadableStream` body must be consumed through the same bounded worker
upload-demand contract as an HTTP(S) request, then reach the deterministic
fixture loader without being rejected as an unsupported streaming body.

## Contract

- Inline dedicated and SharedWorker Fetch accept a bounded `ReadableStream`
  request body for a registered fixture owner.
- The worker realm remains the sole producer. Each body chunk, end, error, or
  cancellation is admitted through one upload demand and preserves reader
  ownership and existing byte/chunk limits.
- The fixture owner consumes the one-shot stream into bounded bytes, then
  invokes the ordinary fixture Fetch path with those bytes as a buffered body.
- Fixture responses remain deterministic registered resources and do not
  inspect, echo, or infer request-body contents.
- HTTP(S) Worker uploads retain the native `reqwest` body stream and true
  transport backpressure; unsupported non-fixture URL schemes remain
  unsupported.

## Implementation

- `javascript.rs` factors the worker upload-demand loop so both network and
  fixture request tasks share worker presence checks, cancellation, terminal
  errors, and event-turn dispatch.
- The HTTP path converts the reusable upload source into the existing
  `reqwest::Body` transport without changing loader-state merge behavior.
- The fixture path collects the source under `MAX_NATIVE_FORM_BODY_BYTES` and
  passes a `NativeRequestBody::Bytes` value to the existing fixture response
  loader, which already admits bounded buffered body-bearing methods.
- The local Worker witness performs buffered and streamed POSTs and verifies
  both receive the registered response.

## Tradeoffs

- Fixture uploads are materialized once because a deterministic fixture has no
  socket consumer. This provides real stream ownership and validation without
  pretending the fixture response can inspect a request body.
- Inline fixture streams therefore pay the same bounded copy as any fixture
  body handoff, while HTTP(S) streams retain direct transport backpressure.
- Response streaming for the fixture response still uses the existing native
  worker response-stream owner; this slice does not broaden complete Streams
  or Web IDL conformance.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_local_worker_fetch_accepts_buffered_and_streamed_fixture_request_bodies --locked -- --nocapture` (1 passed)
- `git diff --check`

The inline Worker witness executes a buffered POST and a two-chunk
`ReadableStream` POST against the same registered fixture URL. Both resolve
with the expected static response, demonstrating that the stream was
consumed by the worker upload bridge and admitted by the fixture owner.
