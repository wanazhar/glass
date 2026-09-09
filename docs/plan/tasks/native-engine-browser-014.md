---
id: native-engine-browser-014
scope: glass-browser/native-engine/network-policy-primitives
status: done
depends-on: [native-engine-browser-013]
---

# BE-02e foundation: shared network-policy primitives

## Objective

Create one typed policy boundary for the remaining network/security work
without pretending that policy storage is the same as resource execution.
Stylesheet loading must use the shared URL and mixed-content checks, while
future script, fetch, image, font, media, frame, and worker callers can use the
same CSP and CORS vocabulary.

## Contract

- CSP response parsing recognizes the bounded source-list families needed by
  the browser profile: `default-src`, `style-src`, `script-src`, `img-src`,
  `font-src`, `media-src`, `frame-src`/`child-src`, `connect-src`, and
  `worker-src`. Unknown directives remain ignored rather than becoming an
  accidental allow rule.
- A typed `NativeSubresourceKind` selects the directive-specific list with
  `default-src` fallback. The existing conservative source matching supports
  `'none'`, `'self'`, `*`, scheme sources, and explicit URL origins.
- Shared subresource URL resolution rejects credentials, keeps only HTTP(S)
  resources eligible, and returns unsupported schemes as an ordinary blocked
  candidate. HTTPS documents reject HTTP subresources at the common
  mixed-content boundary, including stylesheet redirect hops.
- A typed CORS mode distinguishes no-CORS resource fetches from CORS reads.
  Cross-origin CORS requests expose the serialized document origin for the
  request `Origin` header, and a response is readable only with an exact
  `Access-Control-Allow-Origin` match or `*` for non-credentialed access.
  Credentialed reads additionally require
  `Access-Control-Allow-Credentials: true`; same-origin reads do not require
  CORS response headers.
- The stylesheet path now calls the shared resolution, CSP, and mixed-content
  policy functions. It remains a no-CORS resource path, so this batch does not
  incorrectly require `Access-Control-Allow-Origin` for ordinary stylesheets.
- All policy state remains process-owned, bounded, session-only, and absent
  from ordinary logs. There is no hidden CDP or unsandboxed fallback.

## Deliberate boundary and tradeoffs

- This batch does not execute JavaScript, expose a fetch/XHR API, perform CORS
  preflights, load image/font/media/frame/worker bytes, implement CSP nonces or
  hashes, or generate violation reports. Those callers must be added before
  BE-02 can be considered complete.
- The CORS helper validates the response authorization contract but has no
  caller yet. Keeping it private prevents an unimplemented public API from
  being mistaken for browser support.
- Mixed-content handling currently blocks every HTTPS-to-HTTP subresource.
  That is stricter than browser passive-content upgrade rules, but preserves a
  simple fail-closed security invariant until the profile defines upgrades and
  resource-specific behavior.
- CSP duplicate-policy intersection, source paths/ports, nonces/hashes,
  `upgrade-insecure-requests`, report-only policies, and full Fetch preflight
  semantics remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

The batch was checked as one coherent unit:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine --locked`
- `cargo test -p glass-browser --features native-engine --lib resource_loader::tests --locked -- --nocapture` — 7/7
- `cargo test -p glass-browser --features native-engine --test native_engine native_content_process_ --locked -- --nocapture` — 12/12
- `git diff --check`

The next implementation gate is to give these policy primitives real callers:
script/module and connect/fetch request mediation, then remaining resource
classes and service-worker/permission routing.
