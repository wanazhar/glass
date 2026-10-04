---
id: native-engine-browser-837
scope: glass-browser/native-engine/service-worker-registration-referrer-policy
status: in-progress
depends-on: [native-engine-browser-836]
---

# Glass native-engine browser slice 837: ServiceWorker registration referrer policy

## Objective

Apply the active Document's referrer source and policy container to the
ServiceWorker script entry fetches initiated by `register()` and
`ServiceWorkerRegistration.update()`.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-836.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [Service Workers: `register()` and Start Register](https://w3c.github.io/ServiceWorker/#dom-serviceworkercontainer-register)
- [Service Workers: Create Job, Register, and Update algorithms](https://w3c.github.io/ServiceWorker/)
- [HTML Standard: worker script fetching](https://html.spec.whatwg.org/multipage/workers.html#worker-processing-model)
- [Fetch Standard: populating request from client](https://fetch.spec.whatwg.org/#populate-request-from-client)
- [Fetch Standard: main fetch](https://fetch.spec.whatwg.org/#main-fetch)

## Contract

- A registration-entry request initiated by an active Document uses that
  Document's creation URL as its referrer source, even after same-document
  `history.pushState()`/`replaceState()` changes the active URL. Keep the
  current owner URL separate for URL resolution and origin checks. Fetch's
  referrer processing removes the fragment and applies the request policy to
  each target URL.
- Capture the Document's effective policy at the call boundary. This includes
  a valid response `Referrer-Policy` and later valid `meta name="referrer"`
  updates; missing or invalid policy state uses the Document policy-container
  default. The policy is transported with the ServiceWorker register/update
  command rather than reconstructed from a URL-keyed cache entry.
- Cover both classic and module registration roots, plus an explicit
  `ServiceWorkerRegistration.update()` invoked by a live Document. Preserve
  the existing script response policy container for ServiceWorker module
  dependencies and subsequent worker-global Fetch/XHR; the registering
  Document policy controls only the entry request.
- Reject an invalid policy token at the native command boundary with the
  existing typed input error. Do not silently normalize malformed IPC state.
- Do not apply a live-Document policy to user-agent background restoration or
  soft updates with no active client; those request contexts have separate
  referrer provenance.
- A process-backed HTTP regression asserts actual entry-request `Referer`
  headers for response-header `no-referrer`, live meta `origin` and
  `unsafe-url`, and an explicit update after a live policy change. It changes
  the active URL with `pushState()` before registration and verifies that
  `unsafe-url` still exposes only the Document's original creation URL. The
  existing Slice 836 dependency regression remains responsible for proving
  the module worker's later response policy independently.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/service_worker.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-837.md`

## Verification

- `cargo fmt --all -- --check` passed.
- `cargo check -p glass-browser --features native-engine --lib --tests --locked -q`
  passed with existing dead-code warnings from the superseded HTML parser.
- The socket-free runtime test
  `service_worker_entry_commands_capture_live_document_referrer_policy`
  passed (1 passed, 1,652 filtered). It verifies creation-URL retention across
  a same-generation active-URL change, live meta policy capture for
  classic/module registration and explicit update commands, plus rejection of
  an invalid policy token.
- At an earlier checkpoint, the process-backed test's first
  `TcpListener::bind` returned `PermissionDenied` before engine initialization.
  Later process-backed HTTP regressions in Slices 842-843 bound successfully,
  but this specific Slice 837 test has not been rerun. Its updated source
  changes the active URL with `pushState()` and asserts the creation-URL
  `Referer`; scoped compilation is not wire evidence, so this process-backed
  assertion remains open.
- Maintainer documentation gates passed after the final documentation edit:
  release truth audited 1,471 Markdown files (83 current, 63 previous-version
  hits, 1,654 semantic hits, 0 current-claim failures); depth covered 93
  current guides and 19 contracts; shortcut inventory covered 15 implementation
  keys and 63 documentation markers; coverage found 346 full-product MCP tools
  (101 browser-only), 17 examples, and 22 public modules. `cargo fmt --all --
  --check` and `git diff --check` passed. These static gates do not replace the
  still-open process-backed HTTP assertion above.
