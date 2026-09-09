---
id: native-engine-browser-010
scope: glass-browser/native-engine/http-redirect-charset-policy
status: done
depends-on: [native-engine-browser-009]
---

# BE-02a: bounded HTTP redirect and document-encoding policy

## Objective

Harden the first native external-navigation boundary before adding script or
web storage. The shared `NativeResourceLoader` now applies the same strict
redirect policy in the parent and content worker, rejects credential-bearing or
non-HTTP(S) redirects before following them, and decodes declared HTML
charsets for the current bounded document owner.

## Contract

- Every redirect remains within HTTP(S), rejects URL userinfo, and is limited
  to eight hops. A policy violation fails the request before the next URL is
  fetched; there is no filesystem, custom-scheme, or CDP fallback.
- HTTP and HTTPS TLS verification remains reqwest/rustls default validation;
  the final URL is revalidated and its normalized tuple origin is rebuilt
  after redirects. Response status and HTML/XHTML MIME policy remain strict.
- `Content-Type` charset parameters support UTF-8, UTF-16LE/BE (including
  BOM detection), US-ASCII, ISO-8859-1/Latin-1, and Windows-1252 aliases.
  Unknown or invalid encodings fail with a privacy-safe typed network error;
  decoded output remains within the configured document quota.
- The child and parent use the same loader implementation, so process-backed
  external navigation and any future parent-owned path cannot diverge on URL,
  redirect, MIME, size, or charset policy.
- Fragment preservation remains navigation-only metadata: the origin and
  resource fetch URL exclude the fragment, while the committed URL retains the
  caller's fragment after the final redirect.

## Tradeoffs and missed behavior

- Explicit charset support removes the UTF-8-only limitation without adding a
  dependency, but it is not the complete HTML encoding sniffing algorithm. A
  later standards document gate must add prescan/meta/BOM precedence and the
  full WHATWG label table.
- Each navigation still constructs a fresh reqwest client. Cookies, cache,
  HSTS, connection pooling, referrer policy, CORS/CSP, mixed content, service
  workers, permissions, subresources, proxy policy, and WebSocket/EventSource
  remain intentionally unimplemented rather than being implied by this gate.
- Redirect errors do not expose the target URL or response body in diagnostics,
  which protects sensitive data but leaves detailed network tracing for a
  future privacy-safe observability contract.

## Paths

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

The redirect/charset slice was checked and tested as one batch:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine --locked`
- `cargo test -p glass-browser --features native-engine --lib decodes_bounded_declared_html_charsets --locked -- --nocapture`
- `cargo test -p glass-browser --features native-engine --test native_engine native_runtime_session_loads_bounded_external_http_html_without_cdp --locked -- --nocapture`
- `cargo test -p glass-browser --features native-engine --test native_engine native_content_process_decodes_declared_external_html_charset --locked -- --nocapture`

The affected check passed without warnings. Charset unit coverage passed 1/1,
redirected external navigation passed 1/1, and child-backed Windows-1252
external navigation passed 1/1. Cookies/cache, CORS/CSP, mixed content,
service workers, permissions, subresources, full encoding sniffing, WPT,
script execution, and browser promotion remain open.
