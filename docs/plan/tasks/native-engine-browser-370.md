# Native page Fetch response dispatch (370)

```yaml
id: native-engine-browser-370
scope: native-engine/page-fetch-response-transport
status: done
depends-on:
  - native-engine-browser-369
```

## Objective

Remove the page Fetch response continuation's dependency on generated
JavaScript source. A response body admitted by the native resource loader must
reach the page Fetch resolver as structured data instead of being rejected
because the response object exceeds the 16 KiB script-source budget.

## Delivered behavior

- `NativeJavaScriptRuntime::resolve_fetch` validates the request id and enters
  the response payload into QuickJS through a bounded parsed value.
- The installed `__glassResolveFetch` function receives the request id and
  payload directly; no response JSON is interpolated into executable source.
- The page host still evaluates a bounded `undefined;` continuation, so normal
  promise settlement, microtask draining, command collection, storage updates,
  and page mutation handling remain on the existing runtime path.
- The direct payload boundary admits up to the bounded 16 MiB native host
  command envelope, while the resource loader's response, header, and IPC
  limits remain authoritative.
- Local page-event batches continue to dispatch before the Fetch resolver when
  a Fetch continuation and queued page events share one host turn.
- A process-backed HTTP(S) witness proves a 20,000-byte same-origin response
  resolves through `response.text()`; the existing JSON POST/fetch regression
  also remains green.

## Contract and tradeoffs

The response payload is parsed with QuickJS's JSON bridge and remains subject
to the existing JSON-backed response projection, base64 body representation,
opaque filtering, header exposure, and body-stream ownership. The 16 KiB limit
still applies to authored/evaluated JavaScript source; it no longer applies to
the bounded response value that the host supplies to the already-installed
resolver. A 16 MiB envelope bound preserves a finite host-memory/IPC budget,
but does not provide arbitrary-size browser response parity.

This slice covers the page Fetch resolver only. Worker and Service Worker Fetch
response continuations, Fetch stream event payloads, and the remaining
generated network-event paths retain their own bounded contracts and are
separate issue #40 transport/scheduling gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-370.md`

## Verification

The following checks passed locally against the committed slice:

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_fetch_response_delivery_preserves_large_payloads --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_exposes_bounded_same_origin_post_fetch --locked -- --nocapture` — 1 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
