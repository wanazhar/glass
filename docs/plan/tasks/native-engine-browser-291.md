# Glass native engine browser slice 291: module dedicated workers

Status: completed locally.

## Objective

Allow the native browser backend to execute dedicated module workers through
the same isolated worker runtime and policy-owned resource loader used by
classic workers.

## Contract

- `new Worker(url, { type: "module" })` is accepted, while classic workers
  retain their existing behavior and unsupported worker types fail explicitly.
- Static `import`/`export` dependencies and literal dynamic-import URLs are
  prefetched through the worker resource owner with bounded graph entries and
  bytes.
- HTTP(S) and registered fixture module URLs use the existing URL, credentials,
  MIME, mixed-content, `worker-src`, cookie, and response/source-size policy
  boundaries rather than a second network path.
- The root module and each dependency are evaluated by QuickJS in the isolated
  worker realm. Worker message, timer, Fetch, XHR, stream, lifecycle, and
  close/terminate turns remain routed through the existing bounded host queue.
- Module workers have no document and reject `importScripts()`; module
  messages and host responses use fresh module turn identities without
  re-executing the root module.

## Implementation

`NativeWorkerRegistry` now distinguishes classic and module workers. A module
worker graph loader recursively discovers supported import forms, resolves
them relative to the importing module, and stores the normalized source map in
the worker record. `NativeJavaScriptRuntime` evaluates the root and subsequent
worker turns with QuickJS's existing resolver/loader, while the worker
bootstrap keeps the same bounded Web API surface and explicitly disables
`importScripts()` for module workers.

## Tradeoffs and follow-up

Prefetching keeps execution deterministic and prevents module evaluation from
creating an unbounded implicit network capability, but bare specifiers still
need import-map support and computed dynamic imports fail through the bounded
loader. Shared/service/worklet workers, transferables, complete worker Web IDL
parity, and the remaining native/CDP replacement gates are still Issue #40
work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine --locked native_local_module_worker_imports_dependencies_and_handles_messages -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine --locked worker -- --nocapture` (15 passed)
- `git diff --check`

The evidence is local-only. Remote CI, release, registry publication, and
final native/CDP parity or production-promotion claims remain pending the
wider Issue #40 gates.
