# Native engine browser slice 213: decoded image cache

Status: completed locally.

## Objective

Reuse bounded decoded external PNG resources when the same cacheable image is
requested again, while preserving HTTP freshness and privacy policy boundaries.

## Contract

- External image responses use the existing bounded response-cache policy. A
  response with `no-store`, `no-cache`, `max-age=0`, a privacy-sensitive
  `Vary`, or a `Set-Cookie` mutation is not retained in the decoded image
  cache.
- A cacheable successful PNG is keyed by both the requested URL and the final
  URL after the bounded redirect chain. Duplicate sources therefore avoid a
  second network request, including when they share a redirect destination.
- Cache lookup still runs after document URL/CSP/mixed-content checks, so a
  cached resource cannot bypass the current page's subresource policy.
- Decoded entries reuse the existing bounded cache capacity and are cloned
  into each document snapshot. Broken, oversized, unsupported, or denied
  images remain non-fatal and are never cached.

## Implementation

`NativeNetworkState` now owns a bounded decoded `NativeImage` map. The image
loader records response cacheability before consuming the body, rejects
privacy-sensitive cookie responses from storage, and stores successful PNGs
under requested and redirected URL keys. Static duplicate image discovery and
later reactive image hydration share the same loader state, so both paths
benefit from the cache without adding a second resource owner.

## Tradeoffs and follow-up

The cache is deliberately small and deterministic: it shares the existing
bounded entry limit and evicts the lexicographically oldest key rather than
introducing an unbounded LRU policy. A redirect may occupy two keys, which
improves lookup correctness but reduces effective unique-image capacity. The
cache is process-lifetime state and does not persist decoded pixels to the
profile. Freshness validation, request coalescing for concurrent loads,
responsive sources, CSS/SVG image resources, animation, and additional image
formats remain active issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_reuses_cacheable_external_png_for_duplicate_images -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_external_png_through_document_wire -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_loads_image_after_script_source_mutation -- --nocapture` (1 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_blocks_csp_disallowed_image_before_request -- --nocapture` (1 passed, 0 failed)

Implementation checkpoint: `0ecaf537`.
