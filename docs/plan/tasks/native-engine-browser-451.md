# Native engine browser-complete slice 451: general Fetch method tokens

- Status: complete locally
- Scope: `native-engine` / script Fetch method ownership
- Issue: #40
- Depends on: [native-engine-browser-450](native-engine-browser-450.md)

## Objective

Remove the fixed seven-method ceiling from native `Request` and `fetch()` while
keeping document navigation, form submission, history, and XHR policy
boundaries explicit. A normal script Fetch may now carry any bounded HTTP token
accepted by the Fetch method grammar, including extension methods such as
`REPORT`.

## Contract

- Fetch methods are string-owned, normalized to ASCII uppercase, non-empty,
  bounded to 64 bytes, and restricted to HTTP token characters.
- `CONNECT`, `TRACE`, and `TRACK` remain forbidden at the JavaScript and native
  command boundaries.
- `GET` and `HEAD` remain bodyless; `GET`, `HEAD`, and `POST` retain the CORS
  simple-method classification. Other valid tokens use the existing bounded
  preflight, redirect, cookie, cache, and response policy machinery.
- Page, dedicated/SharedWorker, and Service Worker Fetch paths share the same
  native method owner; malformed child-process commands are rejected again.
- Document navigation and target/form submission continue using the closed
  `NativeNavigationMethod` enum, so custom Fetch tokens cannot widen navigation
  or history behavior. XHR retains its current supported-method contract.
- Service Worker Cache request matching accepts valid Fetch method tokens;
  `Cache.put` and `Cache.addAll` remain GET-only as required by their API
  contract.

## Implementation

- Added `NativeFetchMethod` with bounded token validation, forbidden-method
  rejection, reqwest conversion, bodyless/simple classification, and a bridge
  from the existing navigation enum.
- Changed `NativeFetchRequest`, page/worker/content-process pending Fetch state,
  and Service Worker Fetch interception to carry the Fetch-specific owner.
- Generalized SameSite cookie safety checks without changing the navigation
  method policy.
- Replaced page and worker fixed Fetch method arrays with shared bounded token
  validators while keeping separate fixed XHR lists.
- Updated Service Worker Cache command validation to use the same method owner.

## Tradeoffs

The 64-byte bound is an intentional resource and protocol boundary; it is
larger than ordinary registered methods but prevents unbounded command and
preflight-key growth. The implementation does not make XHR arbitrary-method
capable in this slice, because that would change its separate Web IDL and
native sync/async policy surface. Navigation remains closed because treating
an extension Fetch token as a document navigation method would weaken form,
history, redirect, and cookie guarantees.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --lib --locked`
- `cargo check -p glass-browser --tests --locked`
- `cargo check -p glass-browser --test native_engine --locked`
- focused method unit: 1 passed
- cross-origin page Fetch/CORS method family, including `REPORT`: 1 passed
- dedicated-worker fixture Fetch, including `REPORT`: 1 passed
- `git diff --check`

