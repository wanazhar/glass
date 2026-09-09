---
id: native-engine-browser-011
scope: glass-browser/native-engine/http-session-state
status: done
depends-on: [native-engine-browser-010]
---

# BE-02b: bounded HTTP session state and document cache

## Objective

Add the first stateful network behavior to the process-backed native
navigation path without introducing disk persistence, cross-session leakage,
or an unbounded browser cache. The content worker now owns one bounded
network-state instance for its lifetime, so cookies and cache entries survive
normal same-process navigations while remaining isolated from the parent and
from other native sessions.

## Contract

- The content worker creates one NativeResourceLoader after its first load
  and reuses it for later HTTP(S) navigations. The loader's cache and cookie
  jar therefore live with the child content process rather than being
  reconstructed for every request.
- Cookies are session-only and bounded to 128 entries. Names, values, domains,
  paths, Secure, Max-Age, host-only/domain matching, path matching, and
  deletion are handled within the narrow navigation contract. Secure cookies
  are accepted and sent only over HTTPS. Cookie headers are capped at 4 KiB,
  and malformed or oversized values are discarded.
- Cookie values never enter diagnostics, traces, effects, or Debug output.
  No cookie state is written to profiles or otherwise persisted across native
  sessions.
- Successful HTML responses may enter a 32-entry in-memory document cache
  keyed by fragment-free requested URL. Responses with Set-Cookie,
  Cache-Control: no-store|no-cache|max-age=0, Pragma: no-cache, or
  Vary: Cookie|* are not cached. Fragment-only navigation can reuse the
  bounded document while restoring the caller's fragment in the committed URL.
- Cache entries retain only the bounded URL, origin, and decoded HTML body
  already admitted by the document quota. Replacing the oldest deterministic
  key at capacity prevents unbounded growth.
- Parent-owned local/data/fixture loading remains deterministic and does not
  acquire network state. HTTP(S) failures remain typed, bounded, and
  fail-closed; there is no CDP or unsandboxed fallback.

## Tradeoffs and missed behavior

- This is a session cache, not a complete HTTP cache: freshness dates,
  revalidation, validators, Vary negotiation beyond the explicit safety
  deny-list, HSTS, proxy behavior, connection pooling, redirect-cookie
  capture, partitioned cookies, SameSite, and persistent storage remain open.
- The cookie parser intentionally accepts a small safe subset. It does not
  claim full RFC6265bis behavior, public-suffix enforcement, cookie prefixes,
  Expires parsing, or script-visible document.cookie.
- A cacheable response is reused for the session without a freshness clock.
  The bounded policy is useful for deterministic browser work, but the
  standards network gate must replace it with complete cache semantics before
  production promotion.
- Network state is process-owned, which improves containment and lifetime
  correctness but makes child restart a new session boundary. Recovery must
  not replay a failed navigation or silently recreate state from parent data.

## Paths

- crates/glass-browser/src/browser/native_engine/resource_loader.rs
- crates/glass-browser/src/browser/native_engine/content_process.rs
- crates/glass-browser/src/browser/native_engine/engine.rs
- crates/glass-browser/tests/native_engine.rs
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

The stateful network batch was checked as a coherent unit:

- cargo fmt --all -- --check
- cargo check -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine --locked
- cargo test -p glass-browser --features native-engine --lib resource_loader::tests --locked -- --nocapture
- cargo test -p glass-browser --features native-engine --test native_engine native_content_process_ --locked -- --nocapture

The loader policy unit target passed 3/3; the process-backed native target
passed 8/8, including cookie carry-over, fragment cache reuse, charset
decoding, size enforcement, malformed-document atomicity, computed-style
transfer, form mutation, and worker recovery. The test-only worker-environment
injection is serialized so recovery cannot race other process-backed tests.
Full CORS/CSP, mixed-content, service-worker, permission, subresource,
complete cache/encoding, script, WPT, and browser-promotion gates remain open.
