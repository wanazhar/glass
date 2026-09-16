# Native subresource integrity (409)

status: done
scope: native-engine/subresource-integrity
issue: 40

## Objective

Make the native content owner enforce Subresource Integrity (SRI) for
external `script` and stylesheet `link` resources. The integrity decision must
cover raw response bytes, redirects, fresh and revalidated cache entries,
dynamic script insertion, and the CORS boundary required for cross-origin
integrity loads.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-408.md`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- [W3C Subresource Integrity](https://www.w3.org/TR/SRI/)

The native loader already fetched external scripts and stylesheets, but an
`integrity` attribute was ignored. A changed CDN response could therefore be
executed or applied even when the page had pinned the expected bytes. A
response-body hash alone is insufficient for cross-origin SRI: the resource
must also be obtained through an explicit CORS-enabled element fetch, and a
URL-only cache entry cannot stand in for the response headers needed to repeat
that check.

## Contract

- `script[integrity]` and `link[rel=stylesheet][integrity]` parse
  whitespace-separated SRI metadata and recognize SHA-256, SHA-384, and
  SHA-512. The strongest recognized algorithm is selected; multiple metadata
  entries for that algorithm are alternatives.
- Malformed and unknown algorithm tokens are ignored for forward compatibility.
  An integrity string with no recognized metadata behaves as no integrity
  metadata. A recognized digest mismatch fails the resource load.
- Hashing uses the raw response bytes before UTF-8 decoding, CSS parsing, or
  JavaScript evaluation. Empty integrity metadata does not impose a hash.
- Fresh script and stylesheet cache hits, and validated `304` reuse, recheck
  the cached representation against the requested integrity metadata. A
  failed check never executes script or contributes CSS.
- `crossOrigin` reflects the `crossorigin` attribute for `script` and `link`.
  Same-origin integrity loads remain eligible without CORS. A cross-origin
  integrity load requires an explicit `crossorigin` value, sends the document
  `Origin` header, applies anonymous or `use-credentials` response checks, and
  does not reuse a URL-only cache entry whose CORS headers are unavailable.
- Cross-origin `crossorigin` loads without integrity also honor the declared
  CORS mode and credentials boundary. Cross-origin no-CORS subresources do not
  send page cookies.
- Parser-created and dynamically attached external/module scripts carry
  `integrity` and `crossorigin` through the content-process loader. A failed
  integrity check uses the existing resource-error event path and leaves the
  document alive.
- The existing CSP, mixed-content, URL, cookie, redirect, content-type, size,
  and process-boundary owners remain authoritative. SRI never widens CSP or
  turns a report-only policy into enforcement.

## Non-goals

This slice does not add SRI to images, frames, media, or Fetch requests; those
remain separate issue #40 profile work. It does not add future SRI options,
Integrity-Policy headers, or replace the existing bounded HTTP cache with a
header-preserving browser-wide cache. Module dependency requests continue to
use their own resource metadata rather than inheriting the root element's
hash.

## Implementation path

- Add bounded SRI metadata parsing and strongest-hash verification in the
  shared resource loader using the existing SHA-2 and base64 dependencies.
- Thread `integrity` and `crossorigin` from native DOM discovery through
  stylesheet and page-script loading, including mutation-created external
  scripts and reflected JavaScript properties.
- Apply CORS request/response checks and credential suppression at the final
  response boundary, while skipping unsafe cross-origin cache reuse.
- Add loader unit coverage plus same-origin and cross-origin native HTTP
  witnesses for successful, mismatched, and no-CORS resources.
- Synchronize the architecture, active plan, analysis, and task evidence.

## Tradeoffs

- Supporting only SHA-256/384/512 keeps the implementation bounded and uses
  audited existing dependencies; unknown algorithms remain forward-compatible
  rather than silently becoming a weaker accepted hash.
- Verifying cached text bodies is equivalent for the supported text resources
  because they are decoded only after the byte check. Cross-origin CORS loads
  bypass URL-only cache reuse because the current cache does not retain enough
  response-header state to reauthorize another document origin.
- The existing resource-error path preserves page liveness and event ordering,
  but does not expose a new SRI-specific exception type to page script.

## Delivered

- Added SRI metadata parsing, strongest recognized SHA-2 selection, raw-byte
  verification, and cached/`304` verification.
- Added `integrity` and `crossOrigin` reflection and threaded both attributes
  through stylesheet, parser script, module script, and dynamic script loads.
- Added explicit cross-origin CORS request/response enforcement, credential
  handling, and cache isolation for integrity-protected resources.
- Added unit coverage and process-backed HTTP witnesses proving valid style and
  script resources load, mismatches dispatch two normal resource errors, a
  correct cross-origin CORS pair loads, and a correct cross-origin resource
  without `crossorigin` is rejected.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib subresource_integrity --locked -- --nocapture` (1 passed)
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --test native_engine enforces_subresource_integrity --locked -- --nocapture` (1 passed)
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --test native_engine requires_cors_for_cross_origin_integrity_resources --locked -- --nocapture` (1 passed)

Remote CI, push, release, tag, and registry publication remain outside this
local checkpoint.
