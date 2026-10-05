---
id: native-engine-browser-873
scope: glass-browser/native-engine/parent-owned-html-image-preload
status: done
depends-on: [native-engine-browser-872]
---

# Glass native-engine browser slice 873: parent-owned HTML image preload

## Objective

Implement bounded process-backed image preloads for HTTP(S) documents while
preserving the browser parent's exclusive cookie and network authority.

## Context

- `docs/INDEX.md`
- `docs/plan/native-engine-browser-profile.md#parent-owned-cookie-authority`
- `docs/plan/native-engine-browser-profile.md#parent-owned-image-preloads`
- `docs/architecture/native-engine.md` — image resources, process boundary,
  and capability gaps
- `crates/glass-browser/src/browser/native_engine/content_process.rs` —
  parent-brokered image requests and dynamic resource turns
- `crates/glass-browser/src/browser/native_engine/dom.rs` — external resource
  discovery, document-local resource state, and mutation transfer
- `crates/glass-browser/tests/native_engine.rs` — process-backed HTTP fixture
- [HTML Standard: preload](https://html.spec.whatwg.org/multipage/links.html#link-type-preload)
- [Issue #40](https://github.com/wanazhar/glass/issues/40)

## Contract

- Recognize an attached HTML `<link>` whose `rel` token list contains
  `preload`, `as` is `image`, and `href` is nonempty. Perform initial
  preloads before ordinary image requests; rediscover script-inserted links
  and changed `href` values in the owning content turn.
- Resolve preload and image URLs through the same document URL algorithm.
  A completed decoded preload can satisfy only a later image request in the
  same document whose resolved URL matches. Keep this preload cache separate
  from profile state and bound it to 2 MiB decoded pixels and 64 entries.
- Load every HTTP(S) preload through the owner-checked parent image broker.
  Apply the parent image CSP and cookie policy, accept eligible response
  cookies in the parent, and refresh only the current script-visible
  `document.cookie` projection returned to the child. Do not send cookie
  profiles, HttpOnly values, or cookie headers to a content process.
- Dispatch bounded `load`/`error` resource events on the owning link. A cache
  budget miss does not discard a successful load event; a later image then
  uses the ordinary parent broker path.
- Leave a preload inactive when `media`, `type`, `crossorigin`, `integrity`,
  `imagesrcset`, or `imagesizes` is present. Do not claim script/style/font/
  module preload, `modulepreload`, fetch-priority scheduling, complete preload
  cache matching, or standards/WPT conformance.

## Tradeoff

The bounded document-local decoded-image cache allows a matching `<img>` to
reuse even a response that the ordinary HTTP cache cannot retain, without
moving cookie authority into the content process. Its 2 MiB aggregate cap may
leave larger preload sets to make a second parent request; this is explicit
and memory-bounded. The slice adds eager image discovery, not a speculative
parser or concurrent priority scheduler.

## Path

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-873.md`
- `docs/plan/reviews/native-engine-browser-873-01.md`

## Verification

- Exact process-backed HTTP regression passed (1 passed; 934 filtered;
  24.81 seconds). It covers initial and dynamic discovery, duplicate request
  suppression, matching-image reuse despite `no-store`, unsupported CORS
  metadata, load/error delivery and `href` replacement, plus parent-owned
  HttpOnly cookie reuse while `document.cookie` stays filtered.
- Eleven focused preload unit tests and the exact native capability-profile
  unit test passed. The scoped `glass-browser` check completed without new
  diagnostics; existing dead-code warnings remain.
- Rust formatting, `git diff --check`, and all four maintainer documentation
  gates passed. Documentation coverage used existing shared-target binaries.
- A design checkpoint was committed before implementation. The completed
  slice is locally committed with a focused Conventional Commit. No push or
  remote-CI result is claimed. Issue #40 remains open.

## Results

Initial and dynamic HTTP(S) `as=image` preloads use the owner-checked parent
image broker. The parent alone applies image CSP and cookie policy, processes
response cookies, and returns image data plus the URL-scoped script-visible
cookie projection. The document-local decoded-image cache is capped at 2 MiB
and 64 entries. Unsupported preload metadata remains inactive; complete
destination/CORS/integrity matching, responsive selection, other destinations,
priority scheduling, WPT coverage, and cross-platform/remote CI remain open.
