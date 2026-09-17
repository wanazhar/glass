# Native engine browser-complete slice 445: XHR realm state mutators

status: complete
scope: native-engine/xhr-realm-state-mutators
issue: 40
depends-on: [native-engine-browser-444]

## Objective

Align XHR mutator state with the Window-versus-worker contract while keeping
the existing bounded native loader and response owners intact.

## Normative reference

The state and realm rules follow the current [WHATWG XMLHttpRequest
Standard](https://xhr.spec.whatwg.org/), sections 3.5.3, 3.5.4, 3.5.1, and
3.6.8.

## Contract

- `withCredentials` is a private Boolean-backed prototype property. It is
  writable only while the XHR is `UNSENT` or `OPENED` and `send()` has not been
  invoked; later mutation raises `InvalidStateError`. `open()` preserves its
  value for request reuse.
- Page synchronous XHR rejects `timeout` mutation and nonempty
  `responseType` mutation with `InvalidAccessError`. A synchronous `open()`
  also rejects a preconfigured timeout or nonempty response type.
- Worker synchronous XHR may set timeout and supported response types. Setting
  worker `responseType` to `document` is ignored, leaving the previous value
  unchanged.
- Supported response types remain case-insensitively canonical, existing
  bounded numeric timeout validation remains active, and internal Fetch/XHR
  credential selection reads the private Boolean state.

## Implementation

- Replaced page/worker XHR credential data properties with private slots and
  state-gated prototype accessors using realm-local DOM exceptions.
- Added Window synchronous checks to the page timeout and responseType setters
  and to `open()` configuration admission.
- Made the worker document responseType assignment a standards-compatible no-op
  and expanded the existing page/worker response-type witness with credential
  conversion, mutation state, and synchronous configuration assertions.

## Tradeoffs

- The page keeps synchronous XHR available for the bounded existing product
  contract, but enforces the platform's prohibition on timeout and typed
  response configuration in that realm. Workers retain the useful synchronous
  typed-response path.
- `withCredentials` state is preserved across `open()` because it is request
  configuration rather than response state; the loader remains the owner of
  credential, cookie, CORS, and policy decisions.
- This slice does not expand method overloads, URL parsing, or Web IDL
  descriptors; those remain separately auditable issue #40 work.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_response_type_is_canonical_and_state_aware --locked -- --test-threads=1 --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --test-threads=1 --nocapture` (22 passed, 0 failed)
