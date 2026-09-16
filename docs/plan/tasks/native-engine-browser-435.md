# Native engine browser-complete slice 435: fixture buffered request bodies

status: complete
scope: native-engine/fixture-fetch-buffered-request-bodies
issue: 40
depends-on: [native-engine-browser-434]

## Objective

Remove the unnecessary body-method rejection from deterministic fixture Fetch
owners. Bounded buffered request bodies should be accepted for fixture
requests while fixture streams remain explicitly unsupported until a fixture
transport can consume them.

## Contract

- Registered fixture Fetch requests may use body-bearing methods and a bounded
  text or binary request body.
- Existing request-body byte validation, method/body rules, content-type
  validation, same-fixture-host URL checks, and response-size limits remain
  authoritative.
- Fixture responses remain deterministic registered resources; their response
  body does not implicitly echo or inspect the request body.
- A streaming `ReadableStream` request body remains fail-closed for fixtures
  because no fixture-side streaming consumer exists.
- HTTP(S) and Service Worker upload paths retain their existing behavior.

## Implementation

- `resource_loader.rs` removes the fixture-only bodyless-method guard while
  retaining the explicit `reqwest::Body` stream rejection.
- `native_engine.rs` adds a real inline Worker Fetch witness for bounded
  buffered `POST` request data and verifies the static fixture response.

## Tradeoffs

- Fixture tests can now exercise body-bearing Fetch method and metadata
  semantics without starting a network server.
- The body is validated but intentionally has no server-side effect. Claiming
  request inspection would make the deterministic fixture loader misleading.
- Keeping streaming fixture bodies rejected avoids introducing a fake sink or
  silently draining a one-shot stream without a defined owner.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_local_worker_fetch_ --locked -- --nocapture` (3 passed)
- `git diff --check`

The new worker witness sends a bounded `POST` body to a registered fixture
and receives the expected static response. The existing local worker abort
and response-streaming witnesses remain green, including their explicit
stream ownership paths.
