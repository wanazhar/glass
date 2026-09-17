# Native engine browser-complete slice 491: bounded network font cache

- Status: complete
- Scope: `native-engine` / HTTP(S) `@font-face` response reuse
- Issue: #40
- Depends on: [native-engine-browser-490](native-engine-browser-490.md)

## Objective

Avoid repeating an unchanged HTTP(S) font request when a document rebuilds its
stylesheet or reuses the same document-local font source. Preserve the
existing font security boundary while giving the native resource owner the
same bounded freshness and validator behavior already used by other text
subresources.

## Contract

- Network font responses use a dedicated bounded cache with at most the
  existing native cache-entry limit and at most the existing 4 MiB per-face
  payload limit.
- A cache key contains the document origin, canonical requested font URL, and
  the same-origin cookie header used for the request. A font response cached
  for one document origin or cookie state cannot satisfy another origin or
  credential state.
- Fresh cached entries are reused only after the current document's
  `font-src`, mixed-content, URL, and cached final-URL checks pass. File,
  data, Blob, fixture, and unsupported sources do not enter this cache.
- Stale entries send `If-None-Match` and/or `If-Modified-Since` on the initial
  request. A successful 304 reuses the bounded previous bytes and refreshes
  the cache metadata; a redirecting 304 is rejected.
- Only successful non-partial responses with explicit cache metadata are
  stored. `no-store`, `Vary: *`, `Vary: Cookie`, any response `Set-Cookie`,
  empty bodies, invalid/oversized bodies, and failed policy checks remove or
  bypass the entry.
- The cache stores the final response URL with the bytes so later policy
  checks cannot mistake a redirected resource for the originally requested
  target. Invalidated entries fall back to the ordinary bounded network
  loader and never to CDP.

## Implementation

- Add `NativeFontCacheEntry` and a document-origin/cookie-partitioned cache to
  `NativeNetworkState`.
- Reuse the existing response freshness, validator, cache-control, and
  storage-admission helpers for font responses.
- Add fresh-hit, conditional-revalidation, 304, redirect, CORS/CSP, and
  response-size coverage to the network font loader tests.
- Keep the content-process and inline document font-book owners unchanged:
  they receive admitted bytes and continue to rebuild custom faces ahead of
  system faces.

## Tradeoffs and remaining scope

The cache is in-memory and process-local, so it is bounded and cheap to
discard but is not a disk-persistent or cross-content-process cache. Entries
without explicit freshness/validator metadata are not retained, which avoids
inventing a default lifetime but may refetch some valid fonts. Cookie state is
part of the key rather than relying only on `Vary`, trading a few duplicate
entries for safer credential isolation. `local()` lookup, FontFace/
FontFaceSet loading events, font-display timing, variable/color fonts, format
descriptors, language/script matching, mixed bidi/writing modes, and complete
browser text/Web IDL parity remain issue #40 gates.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test --quiet -p glass-browser --lib --locked font_` (18 passed)
- fresh network-font cache reuse and ETag/304 revalidation witnesses pass
- process/listener audit found no stale Glass, Cargo, rustc, Chromium,
  Firefox, or native-content-worker targets
- documentation truth/depth/shortcut/coverage audits
- `git diff --check`

The implementation is local-only at this checkpoint: it is not pushed, run in
remote CI, released, tagged, or published.
