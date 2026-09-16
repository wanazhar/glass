# Native ReadableStream transfer (391)

```yaml
id: native-engine-browser-391
scope: native-engine/readable-stream-transfer
status: done
depends-on:
  - native-engine-browser-390
```

## Objective

Make the native `ReadableStream` surface transferable through the existing
bounded structured-clone owner. A stream transferred with
`postMessage()` or `structuredClone(value, { transfer })` must remain
demand-driven across same-realm and page/worker boundaries instead of being
eagerly flattened into a byte snapshot.

## Contract

- A `ReadableStream` is admitted only when it appears in the transfer list;
  omitted, locked, duplicate, or malformed members fail with
  `DataCloneError` without committing the source or its transport endpoint.
- The transfer descriptor carries only validated mode and an internal,
  hidden bridge-port index. The transport endpoint is never exposed through
  the receiving `MessageEvent.ports` list.
- A receiving page or worker gets a fresh native stream object. Default streams
  preserve structured-cloneable chunk values; byte streams preserve byte
  chunks and byte-mode identity, with bounded BYOB handling in the worker
  receiver.
- The source is locked only after clone admission succeeds. A receiving read
  sends one bounded `pull` control at a time; chunks, close, errors, and cancel
  travel through the same owner-routed MessagePort path.
- Queue, chunk, message, script, and realm limits remain enforced by the
  existing native message and stream limits. No CDP or eager whole-stream
  buffering path is introduced.

## Delivered behavior

- Added readable-stream transfer markers, mode hooks, descriptors, and
  receiver validation to the shared tagged structured-clone graph. Decoding
  now receives the complete reconstructed port table, including hidden
  internal endpoints.
- Added page and worker demand-driven stream bridges. Page receivers support
  default and byte streams; worker receivers preserve default structured
  values as well as byte streams and reject BYOB readers for non-byte streams.
- Kept internal bridge endpoints out of `MessageEvent.ports` while retaining
  normal user-transferred ports in that list. Source readers are installed
  only after the clone graph has been admitted.
- Added a local structured-clone witness and a page-to-worker witness for
  byte and default streams. The witness verifies source locking, receiver
  identity, hidden transport ports, ordered chunks, close delivery, and
  bounded multi-turn progress through the existing host scheduler.

## Tradeoffs and explicit follow-up

The JSON-framed implementation copies each bounded chunk through the existing
structured-clone graph. This protects realm ownership and keeps the wire
format deterministic, but it is not zero-copy and requires multiple host
turns for a pull/chunk/close exchange because the current scheduler is
operation-boundary driven. The stream queue remains bounded rather than
providing an autonomous resident event loop. Full worker-created underlying
source/Web IDL parity, remote-stream tee parity, upload backpressure,
cross-source task ordering, broader transferables, and final production
certification remain issue #40 gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-391.md`
- `crates/glass-browser/README.md`

## Verification

Validation completed locally against this checkpoint:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_readable_stream_transfer_preserves_pull_order_and_hides_transport_port --locked` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_message --locked` — 2 passed
- `cargo test --quiet -p glass-browser --test native_engine native_local_message --locked` — 2 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_transfers --locked` — 3 passed
- documentation/release, depth, shortcut, and coverage validators after the docs update
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
