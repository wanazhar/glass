---
id: native-engine-browser-012
scope: glass-browser/native-engine/http-origin-referrer-policy
status: done
depends-on: [native-engine-browser-011]
---

# BE-02c: explicit origin and referrer policy for native navigation

## Objective

Make the HTTP(S) request boundary explicit and auditable before adding
subresource or script fetches. Native top-level navigation remains permitted
across origins, but the request referrer is derived from the committed
document and recomputed for every redirect hop. The parent and child share
the same normalization and URL policy.

## Contract

- Same-origin navigation sends the fragment-free full URL as the Referer
  header. Cross-origin navigation sends only the source origin's ASCII
  serialization. HTTPS-to-HTTP downgrade and transitions from opaque local
  documents send no referrer.
- The engine derives the referrer from the previously committed URL; callers
  cannot inject arbitrary referrer text through the public navigation path.
  The child validates the optional IPC referrer again before placing it on a
  request, and malformed/non-HTTP(S)/credential-bearing values fail closed or
  become no-referrer according to the explicit policy.
- Automatic reqwest redirect following is disabled. The loader owns a manual
  GET redirect loop so each hop validates its location, HTTP(S) scheme,
  userinfo absence, and eight-hop bound before sending the next request.
  Each hop recomputes strict-origin-when-cross-origin referrer policy.
- Redirect response cookies are collected as bounded pending state and are
  committed only when the final HTML document has passed status, MIME,
  transfer-size, charset, and decoded-size checks. A failed navigation does
  not partially commit redirect state.
- Top-level cross-origin navigation is an allowed browser transition. This
  slice does not claim that arbitrary cross-origin subresource, fetch, XHR,
  WebSocket, or script requests are allowed; those require the upcoming
  CORS/CSP and subresource mediation gates.
- No fragments, credentials, response bodies, cookie values, or referrer
  values enter diagnostics, traces, effects, or ordinary debug output.

## Tradeoffs and missed behavior

- The policy implements the modern strict-origin-when-cross-origin shape for
  this document-only GET path, not every selectable Referrer-Policy token,
  origin-agent-cluster rule, site-for-cookies rule, or browser navigation
  edge case.
- HTTP cache freshness/revalidation, HSTS, CSP, mixed-content blocking,
  CORS preflights, Origin-header generation for script/fetch, subresources,
  service workers, permissions, and WebSocket/EventSource mediation remain
  open. The current cross-origin decision is intentionally limited to
  top-level document navigation.
- Manual redirects add a small amount of request-loop code and may cost
  connection reuse compared with a fully featured client redirect stack, but
  they make per-hop security and state ownership observable and prevent a
  full referrer from leaking across an origin-changing redirect.

## Paths

- crates/glass-browser/src/browser/native_engine/resource_loader.rs
- crates/glass-browser/src/browser/native_engine/content_process.rs
- crates/glass-browser/src/browser/native_engine/engine.rs
- crates/glass-browser/tests/native_engine.rs
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

The origin/referrer batch was checked as a coherent unit:

- cargo fmt --all -- --check
- cargo check -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine --locked
- cargo test -p glass-browser --features native-engine --lib resource_loader::tests --locked -- --nocapture
- cargo test -p glass-browser --features native-engine --test native_engine native_content_process_ --locked -- --nocapture
- cargo test -p glass-browser --features native-engine --test native_engine native_runtime_session_loads_bounded_external_http_html_without_cdp --locked -- --nocapture

The loader policy target passed 4/4; the process-backed origin/referrer and
state subset passed 10/10; and the redirected external-navigation regression
passed 1/1. CORS/CSP, mixed content, service workers, permissions,
subresources, complete HTTP cache semantics, script, WPT, and browser
promotion remain open.
