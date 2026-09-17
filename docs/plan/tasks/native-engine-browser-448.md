# Native engine browser-complete slice 448: canonical URL resolution

status: complete
scope: native-engine/url-canonical-resolution
issue: 40
depends-on: [native-engine-browser-447]

## Objective

Make page and dedicated/SharedWorker URL construction and relative resolution
use the same bounded, canonical URL parser as the native resource and policy
owners. URL inputs must not be interpreted differently by a JavaScript realm,
an inline owner, or the HTTP(S) content process.

## Normative reference

The URL resolution and serialization boundary follows the [WHATWG URL
Standard](https://url.spec.whatwg.org/). Fetch and XHR continue to consume the
result through their existing [WHATWG Fetch
Standard](https://fetch.spec.whatwg.org/) ownership and Glass's explicit
bounded transport and policy rules.

## Contract

- Page and dedicated/SharedWorker `URL` construction resolves absolute,
  relative, scheme-relative, special-scheme, and opaque inputs through the
  native Rust URL parser. The resulting URL is canonical before the existing
  JavaScript URL projection exposes `href`, origin, authority, path, query, or
  fragment fields.
- Canonicalization applies standard URL details that the previous local
  string parser missed, including special-scheme handling, default-port
  removal, host normalization, path/query/fragment escaping, and dot-segment
  resolution. Invalid URL input remains a JavaScript `TypeError`.
- URL objects remain realm-owned JavaScript objects with their existing
  bounded mutable properties and `URLSearchParams` synchronization. This
  slice does not claim complete URL setter, URL Web IDL descriptor, or every
  browser scheme behavior.
- The canonicalizer is installed before page, worker, and module bootstraps;
  `Request`, `fetch()`, XHR, resource attributes, workers, history, and other
  consumers therefore share the same URL boundary without adding a second
  transport or security-policy owner.

## Implementation

- Added a synchronous Rust host source backed by the existing `url` crate,
  with bounded input and base URL sizes and `Url::options().base_url(...)`
  resolution.
- Installed that source for page evaluation, dedicated/SharedWorker
  evaluation, and page module evaluation before the corresponding bootstrap
  constructs `location` or worker URL state.
- Routed page and worker URL resolution through the host source while
  retaining their separate realm projections and existing mutation behavior.
- Added a native integration witness for escaped spaces, default ports,
  scheme-relative URLs, special-scheme URLs with and without a base, opaque
  URLs, and invalid hosts; the existing URL, Fetch, and XHR families cover
  worker and transport regressions.

## Tradeoffs

- Reusing the native Rust URL owner removes parser drift between JavaScript,
  navigation, resource, cookie, CORS, and CSP paths, at the cost of one
  bounded synchronous host call for each initial URL construction or
  resolution.
- Canonical serialization intentionally changes observable results for
  previously accepted noncanonical input, such as explicit default ports and
  unescaped URL components. This is required for stable ownership but means
  callers that compare raw input strings must use `href` as the canonical
  value.
- The JavaScript URL objects still need later conformance work for complete
  setter algorithms, descriptor identity, live `URLSearchParams` edge cases,
  blob/file URL origin rules, and the full browser scheme matrix.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_local_url_resolution_uses_canonical_url_parser --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine url --locked -- --test-threads=1 --nocapture` (7 passed, 0 failed)
- `cargo test --quiet -p glass-browser --test native_engine fetch --locked -- --test-threads=1 --nocapture` (37 passed, 0 failed)
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --test-threads=1 --nocapture` (22 passed, 0 failed)
- `git diff --check`
