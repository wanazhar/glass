---
id: native-engine-browser-830
scope: glass-browser/native-engine/stylesheet-link-referrer-policy
status: in-progress
depends-on: [native-engine-browser-829]
---

# Glass native-engine browser slice 830: stylesheet-link referrer policy

## Objective

Apply the owning Document and `HTMLLinkElement.referrerPolicy` policy to
network stylesheet requests initiated by parser-discovered or dynamically
inserted HTML `<link rel="stylesheet" href>` elements.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-829.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [HTML Standard: `link` element](https://html.spec.whatwg.org/multipage/semantics.html#the-link-element)
- [HTML Standard: referrer-policy attributes](https://html.spec.whatwg.org/multipage/urls-and-fetching.html#referrer-policy-attributes)
- [Referrer Policy: redirect processing](https://w3c.github.io/webappsec-referrer-policy/#integration-with-fetch)

## Contract

- For a network stylesheet request initiated by an HTML stylesheet link,
  recognize all eight standard Referrer Policy tokens ASCII-case-insensitively.
  Missing, empty, and invalid values use the owning live Document's current
  response-header or meta-derived policy.
- `HTMLLinkElement.referrerPolicy` reflects the content attribute as a
  limited-known-values string. Invalid attribute text reads as the empty
  string. Setting the property updates the attribute, and a subsequent fetch
  observes it; it does not rewrite an already-dispatched request.
- Apply the effective policy to parser-discovered and dynamically inserted
  external stylesheet requests. Keep stylesheet `@import`, font/image
  subresources, icons, preloads, other link relationships, workers, and other
  request initiators on their existing paths.
- Compute every outgoing `Referer` from the original owning Document URL under
  the effective policy. A recognized `Referrer-Policy` response header on a
  redirect updates the policy for the following hop; absent or unrecognized
  values leave it unchanged.
- Preserve URL resolution, CSS/CSP/mixed-content checks, CORS, integrity,
  cookies, MIME checks, redirect/byte limits, script/style event ordering,
  stylesheet cache keys/freshness, and style application. A cache hit performs
  no request; conditional revalidation uses the current request's effective
  Referer.
- A process-backed two-origin HTTP regression verifies actual headers for
  same-/cross-origin parser and dynamic requests, response-header and live-meta
  defaults, IDL reflection/mutation, redirect policy updates, cache
  revalidation, and successful style application.
- This slice does not establish `@import` policy inheritance, complete
  Referrer Policy/Fetch or WPT conformance, platform certification, or browser
  completion.

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-830.md`

## Verification

- `cargo check -p glass-browser --features native-engine --lib --tests --locked -q` passed after the implementation and regression were added. It found a missing default referrer policy in a test-only legacy `NativeDocument` constructor; that constructor now initializes to the standard `strict-origin-when-cross-origin` policy.
- `cargo test -p glass-browser --lib --features native-engine referrer --locked --quiet` passed (7 passed, 1,637 filtered; 8.07 seconds), including the focused element override, invalid-value fallback, response-header default, and parsed meta default test plus the existing token/referrer computation tests.
- The process-backed two-origin regression is implemented and type-checks, but could not run in this environment: `TcpListener::bind("127.0.0.1:0")` returned `PermissionDenied` before any HTTP request or browser assertion. Its wire-level behavior is therefore unverified locally, not passed.
- `cargo fmt --all` and `git diff --check` passed for the slice.
- Remote CI, WPT conformance, and cross-platform certification are outside this
  slice and must not be claimed from local results.
