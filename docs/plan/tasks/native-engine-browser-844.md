# Native engine browser slice 844: live SVG image subresources

```yaml
id: native-engine-browser-844
scope: native-engine
status: done
depends-on: [native-engine-browser-211, native-engine-browser-228]
```

## Objective

Load and paint bounded image resources referenced by `<image href>` and
`<image xlink:href>` inside a live inline SVG document. Reuse the existing
image owner and parent fetch broker; do not give the content process a cookie
jar or direct network authority.

## Context

- `docs/architecture/native-engine.md` — native SVG geometry, image-resource
  ownership, typed process boundary, and parent-owned network policy.
- `docs/plan/native-engine-browser-profile.md` — GCWP-0.1 rendering coverage
  and parent-owned cookie authority.
- `docs/plan/tasks/native-engine-browser-211.md` — external image loading and
  typed resource transfer.
- `docs/plan/tasks/native-engine-browser-228.md` — standalone SVG image
  decoding and its external-subresource restriction.
- `docs/plan/tasks/native-engine-browser-843.md` — ongoing parent cookie
  authority work and its prohibition on child-side cookie/network fallback.

## GCWP-0.1 mapping

- The `rendering` domain requires images/SVG, transforms, clipping, hit
  testing, software surfaces, and screenshots; this slice connects live SVG
  image geometry and pixels to those existing owners.
- The `Parent-owned cookie authority` contract requires the parent to select
  request cookies and accept `Set-Cookie`, while content processes receive no
  cookie jar, HttpOnly value, or direct HTTP(S) fallback. This slice reuses
  that authority for SVG image requests.
- The `network-origin` domain requires CSP and mixed-content policy; requests
  reuse the existing checked parent loader, with CSP denial covered before
  dispatch.

## Contract

- Only `<image>` nodes attached beneath a live SVG root participate. Resolve
  `href` first and `xlink:href` as the legacy fallback; source changes replace
  the prior resource identity so stale pixels are never painted.
- HTTP(S) image loads use the existing parent fetch broker, image decoder,
  profile cookie owner, and request security checks (including CSP, redirect,
  mixed-content, and referrer policy). The parent selects request cookies and
  applies response `Set-Cookie`; `HttpOnly` values remain hidden from
  `document.cookie`.
- The content process receives only bounded image-resource results over the
  existing typed wire. It does not own cookie state or open an alternate
  network path. Local data-image sources use the existing bounded decoder and
  do not issue network requests.
- Finite positive SVG image viewport dimensions and supported transforms map
  the decoded image into the existing clipping, display-list, software-raster,
  and capture path. Invalid or over-budget data fails closed.
- Network failure or policy denial leaves a non-fatal broken image; unrelated
  page rendering continues.
- External resources referenced from an SVG document decoded as an image
  remain blocked. This task covers image subresources in an active inline SVG,
  not recursive external fetches from standalone SVG image documents.

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/layout.rs`
- `crates/glass-browser/src/browser/native_engine/paint.rs`
- `crates/glass-browser/src/browser/native_engine/raster.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-844.md`

## Verification

- Scoped `cargo check -p glass-browser --features native-engine --lib --test native_engine --locked`.
- Focused integration test verifies parent-cookie selection, HttpOnly
  invisibility, response-cookie persistence across subsequent SVG image loads,
  typed image transfer, and rendered pixels.
- Focused tests cover `href`/`xlink:href`, local data resources, source
  replacement, and CSP denial before a network request.
- `cargo fmt --all -- --check` and `git diff --check`.

Verification completed locally:

- `cargo fmt --all -- --check` — passed.
- `cargo check -p glass-browser --features native-engine --lib --test native_engine --locked --quiet` — passed.
- `cargo test -p glass-browser --features native-engine --test native_engine --locked --quiet native_content_process_loads_live_svg_images_through_parent_cookie_authority -- --exact` — 1 passed.
- `cargo test -p glass-browser --features native-engine --test native_engine --locked --quiet native_content_process_blocks_csp_disallowed_image_before_request -- --exact` — 1 passed.
- `git diff --check` — passed.

The complete Issue #40 browser profile and native-only promotion gates remain
active after this slice.
