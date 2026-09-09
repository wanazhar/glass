---
id: native-engine-browser-013
scope: glass-browser/native-engine/http-csp-stylesheet-subresources
status: done
depends-on: [native-engine-browser-012]
---

# BE-02d: CSP, mixed-content, and initial stylesheet subresources

## Objective

Open the first bounded subresource path for external native documents without
allowing arbitrary network fetches. The child discovers link stylesheets,
mediates each candidate against the owning document's CSP and mixed-content
policy, fetches only validated CSS, and reparses the document with the
accepted stylesheet sources before transferring the snapshot.

## Contract

- The child performs one discovery parse, then examines at most 16
  link elements whose rel token contains stylesheet. The aggregate fetched
  stylesheet source is capped at 512 KiB, and each response remains subject
  to the configured document-size limit.
- Relative stylesheet URLs resolve against the final document URL. Only
  HTTP(S) targets without credentials are eligible. Unsupported schemes and
  HTTPS-to-HTTP stylesheet requests are blocked before network I/O.
- The document's Content-Security-Policy header supports the bounded
  style-src/default-src source-list needed by this slice: self, wildcard,
  explicit scheme sources, explicit origin sources, and none. A disallowed
  stylesheet is blocked before request; a stylesheet redirect is checked
  again against the same policy and mixed-content rule at every hop.
- Stylesheet requests use the same bounded cookie, redirect, referrer, and
  response-size policy as navigation. They require a text/css response and
  bounded UTF-8 CSS. Redirect cookies are committed only after the CSS body
  passes status, MIME, size, and decoding checks.
- Same-origin stylesheet requests send the fragment-free document URL as
  referrer; cross-origin stylesheet requests send only the document origin.
  The CSS response is not exposed as raw evidence or logged.
- Accepted stylesheets are included in a child-owned reparse before computed
  styles are transferred. The parent remains the sole document commit owner.
- This slice does not enable script, fetch/XHR, images, media, fonts,
  @import, WebSocket/EventSource, or arbitrary cross-origin reads. CORS for
  those request classes remains a later gate; stylesheet loading uses the
  browser's no-cors-style resource behavior and CSP/mixed-content checks.

## Tradeoffs and missed behavior

- Discovery currently reparses the bounded HTML once before stylesheet fetch
  and once after, which keeps resource ownership simple but adds CPU for
  every external page. A standards parser/resource graph can remove that
  duplicate work later.
- CSP source matching is intentionally narrow and conservative. Non-ASCII
  headers, unsupported source expressions, duplicate-policy intersection,
  nonce/hash style authorization, report-only policy, upgrade-insecure-
  requests, frame/worker directives, and full CSP violation reporting remain
  open.
- CSS is decoded as bounded UTF-8 and external rule ordering is appended after
  inline style sources. Full CSS encoding, link-order cascade semantics,
  @import recursion, media attributes, stylesheet disabling, and preload
  behavior remain outside this slice.
- A stylesheet network failure aborts the document load rather than silently
  presenting an unstyled page. This conservative fail-closed behavior is
  explicit for the experimental boundary and should be revisited with
  standards error-event semantics.

## Paths

- crates/glass-browser/src/browser/native_engine/dom.rs
- crates/glass-browser/src/browser/native_engine/resource_loader.rs
- crates/glass-browser/src/browser/native_engine/content_process.rs
- crates/glass-browser/tests/native_engine.rs
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

The stylesheet-subresource batch was checked as a coherent unit:

- cargo fmt --all -- --check
- cargo check -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine --locked
- cargo test -p glass-browser --features native-engine --test native_engine native_content_process_ --locked -- --nocapture

The process-backed native filter passed 12/12, including CSP-allowed
same-origin stylesheet application, CSP blocking before cross-origin request,
origin/referrer policy, cookies/cache, redirects, charset, limits, malformed
document atomicity, computed-style transfer, form mutation, and worker
recovery. CORS/fetch/XHR, mixed-content wire coverage over TLS, scripts,
images/media/fonts, service workers, permissions, full CSP, WPT, and browser
promotion remain open.
