---
id: native-engine-browser-828
scope: glass-browser/native-engine/img-referrer-policy
status: complete
depends-on: [native-engine-browser-827]
---

# Glass native-engine browser slice 828: image referrer policy

## Objective

Apply the effective Referrer Policy to network requests initiated by HTML
`<img>` elements, including the element's `referrerpolicy` content attribute
and `HTMLImageElement.referrerPolicy` reflection.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-827.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [HTML Standard: `img` referrer policy](https://html.spec.whatwg.org/multipage/embedded-content.html#the-img-element)
- [HTML Standard: referrer policy attributes](https://html.spec.whatwg.org/multipage/urls-and-fetching.html#referrer-policy-attributes)
- [Referrer Policy: redirect processing](https://w3c.github.io/webappsec-referrer-policy/#integration-with-fetch)

## Contract

- For a network image request, the selected `<img>` element owns the
  element-level policy, including when its URL was selected through `srcset`
  or a `<picture>` source. A `<source>` candidate does not replace the
  requesting `<img>` as the policy owner.
- Recognize all eight Referrer Policy tokens ASCII-case-insensitively.
  Missing, empty, or invalid `referrerpolicy` values use the owning live
  Document's current policy, including its response-header seed and parsed or
  live `meta name="referrer"` changes. Do not consult URL-keyed shared state
  for a live Document's meta value.
- `HTMLImageElement.referrerPolicy` reflects the content attribute and is
  limited to known values. Setting it must flow through the existing DOM
  mutation path so an image fetch started afterward observes the new value.
  Changing the policy does not rewrite an already dispatched request.
- Compute each outgoing `Referer` from the original Document URL under the
  request's effective policy. Strip the fragment and credentials according to
  the existing Fetch referrer helper. A recognized `Referrer-Policy` header on
  a redirect response updates the request policy for the following hop; an
  unknown or absent token leaves the current request policy unchanged.
- Preserve image URL selection, CSP, mixed-content, cookie, image-cache,
  redirect-count, byte, decode, event, and error behavior. Blob/data/local
  image sources do not emit network requests.
- A process-backed local HTTP regression verifies the real outgoing
  `Referer` header for same-origin and cross-origin requests, element override
  and Document fallback, invalid/missing values, IDL mutation before dispatch,
  and redirect policy updates.
- This slice does not cover navigation, links, scripts, stylesheets, media,
  workers, all other element request initiators, broad Fetch/Referrer Policy
  WPT conformance, or browser-completion gates.

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-828.md`

## Verification

- `cargo fmt --all -- --check` passed.
- `cargo check -p glass-browser --features native-engine --test native_engine --locked --quiet` passed. The initial check caught a moved test-fixture string; cloning it before the spawned server fixed the sole diagnostic. Existing dead-code warnings from the superseded HTML parser remain.
- `cargo test -p glass-browser --features native-engine --test native_engine native_content_process_applies_img_referrer_policy --locked --quiet -- --exact --nocapture` passed (1 passed, 876 filtered; 23.62 seconds). The two local HTTP servers observed the same-origin image Referer, default cross-origin fallback, case-insensitive `ORIGIN`, `no-referrer`, invalid-value fallback, `unsafe-url`, redirect response policy update, and a dynamically created image configured through the `referrerPolicy` IDL property.
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-828.json` passed: 1,456 Markdown documents, 83 current documents, zero current-claim failures.
- `python3 scripts/check-documentation-depth.py` passed: 93 current guides routed/audited and 19 substantive contracts.
- `python3 scripts/check-tui-shortcuts.py` passed: 15 implementation keys and 63 documentation markers.
- `python3 scripts/check-documentation-coverage.py` passed: 1,456 Markdown files, 346 full-product MCP tools (101 browser-only), 17 examples, and 22 public modules.
- `git diff --check` passed.
- Remote CI, WPT conformance, and cross-platform certification were not run or claimed.
