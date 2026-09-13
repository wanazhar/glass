# Glass native engine browser slice 286: worker Fetch

Status: completed locally.

## Objective

Give classic dedicated workers a real, host-owned Fetch path so ordinary worker
code can retrieve HTTP(S) resources and continue in its own promise realm. The
page realm and worker realm must remain separate, while both use the same
resource and security owner.

## Contract

- Worker `fetch()` accepts bounded URL text, the supported HTTP methods, string
  request bodies, custom request headers, credentials, CORS mode, and redirect
  mode.
- Worker requests carry an explicit worker owner ID and are never eligible for
  page Fetch resolution or page DOM mutation.
- The worker registry routes requests through the existing native loader. URL
  resolution, mixed-content checks, connect policy, CORS/preflight rules,
  redirects, cookies, response limits, and transport errors stay host-owned.
- Responses expose bounded status, URL, redirect, opaque, content-type, header,
  `text()`, `json()`, `body`, and `bodyUsed` behavior inside the worker realm.
- Response settlement re-enters the isolated worker promise queue; callbacks
  may post a structured-cloned result to the owning page.
- Startup, worker-message, and due-timer turns use the same request/response
  handoff. A network failure rejects the worker promise without silently
  falling back to CDP or a page resolver.
- Request and response transfers remain bounded by the existing script,
  header, body, response, and worker-message limits.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- Issue [#40](https://github.com/wanazhar/glass/issues/40)

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Implementation

The Fetch command now has an optional worker owner ID, preserving the existing
page command wire shape when the page emits it. The worker bootstrap installs a
bounded Fetch/response surface and records pending promises in the persistent
worker realm. The registry validates the owner, converts the typed command to
`NativeFetchRequest`, awaits the shared loader, serializes the bounded response
metadata/body, and evaluates a worker-only resolver script so pending promise
callbacks run before messages are returned to the page turn.

Worker timer and post-message delivery are asynchronous at the registry
boundary, allowing a Fetch issued from any of those turns to use the same
loader and response settlement path. The content-process and local owners both
await that path. Page Fetch collection ignores the optional worker marker, and
worker command validation rejects missing or mismatched ownership.

## Tradeoffs and follow-up

This slice deliberately ships a useful worker Fetch core rather than copying
the much larger page Fetch implementation into the worker bootstrap. Worker
responses are bounded text-backed objects with `text()` and `json()` consumers;
worker ReadableStream bodies, binary-preserving request/response consumers,
full Request/Response Web IDL identity, AbortController integration, XHR,
WebSocket/EventSource, service workers, module/shared workers, transferables,
and exact browser task-source scheduling remain later Issue #40 work. The
tradeoff keeps one Rust network policy owner and avoids a second divergent
security implementation, while making the current supported path observable
and testable end to end.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet --locked -p glass-browser --features native-engine --test native_engine native_content_process_worker_fetch_resolves_inside_worker_realm -- --nocapture` (1 passed)
- `cargo test --quiet --locked -p glass-browser --features native-engine --test native_engine worker -- --nocapture` (12 passed)
- `git diff --check`

The evidence is local-only. Remote CI, registry publication, release, and final
native/CDP parity or production-promotion claims remain pending the wider Issue
#40 gates.
