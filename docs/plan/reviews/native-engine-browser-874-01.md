# Independent Review: Native Engine Browser Slice 874

- Task: `docs/plan/tasks/native-engine-browser-874.md`
- Reviewed commit: `b327f6e28bc63dcbeb729bbca575b625302a6974`
- Checkout: `task/native-engine-browser-874-verify`
- Review scope: documentation contracts and all task-path implementation/test files: `content_process.rs`, `dom.rs`, `javascript.rs`, `layout.rs`, `paint.rs`, `resource_loader.rs`, and `tests/native_engine.rs`.

## Findings

### [P2] [blocking] Eligible `Set-Cookie` is discarded when the poster response fails

**Contract:** The task requires the parent to accept eligible response cookies (`docs/plan/tasks/native-engine-browser-874.md:49-53`); the profile requires eligible response cookies to remain parent-owned (`docs/plan/native-engine-browser-profile.md:539-551`), consistent with the parent-only authority contract (`docs/architecture/native-engine.md:752-776`).

**Evidence:** `crates/glass-browser/src/browser/native_engine/resource_loader.rs:6915-6922` saves response `Set-Cookie` values in `pending_cookies`. They are committed only on a usable cached response (`:6980-6983`) or after successful image decoding (`:7025-7032`). The intervening redirect-limit/location/policy exits (`:6927-6957`), non-success status (`:6998-7000`), unsupported MIME/oversize response (`:7001-7008`), and decode failure (`:7025-7027`) return without accepting them. Thus even an otherwise eligible cookie on a 404 or undecodable image is lost; cookies received on a redirect are also lost if a later redirect is rejected or the final image fails. The process-backed test covers a successful `Set-Cookie` and later reuse (`crates/glass-browser/tests/native_engine.rs:74941-74965`), but its 404 response has no `Set-Cookie` (`:74968-74977`).

**Required before pass:** Process each response's eligible cookies in the parent as that response is received, independently of image status/body/decode success, while preserving the existing child boundary. Add a loopback regression where a failed poster response sets a cookie and a subsequent request demonstrates that the parent accepted it; verify an HttpOnly value remains absent from `document.cookie`.

### [P2] [blocking] A nonstandard video attribute can override the Document referrer policy

**Contract:** The task specifies that the owner Document is the request client and supplies its effective referrer policy (`docs/plan/tasks/native-engine-browser-874.md:45-49`; profile `docs/plan/native-engine-browser-profile.md:539-548`). The [HTML Standard's video poster algorithm](https://html.spec.whatwg.org/multipage/media.html#the-video-element) does not define a per-video `referrerpolicy` override for poster fetching.

**Evidence:** `crates/glass-browser/src/browser/native_engine/dom.rs:5082-5095` reads `referrerpolicy` from the `<video>` element and uses it in preference to `document_referrer_policy`. A page can therefore set an arbitrary `video[referrerpolicy]` value such as `unsafe-url` and override the Document's effective policy, contrary to the task contract. No test distinguishes the Document policy from a `referrerpolicy` attribute on the video.

**Required before pass:** Derive the poster request policy from the owner Document only. Add a process-backed test that sets a restrictive Document policy and a conflicting video attribute, then asserts the actual `Referer` sent on the poster request.

### [P2] [blocking] Poster URLs are resolved against the document URL, not the Document base URL

**Contract:** Initial and mutated poster values must be resolved against the owner Document (`docs/plan/tasks/native-engine-browser-874.md:39-42`; profile `docs/plan/native-engine-browser-profile.md:539-545`). The [HTML Standard](https://html.spec.whatwg.org/multipage/media.html#the-video-element) resolves the poster URL relative to the video node's Document, which uses that Document's base URL.

**Evidence:** `crates/glass-browser/src/browser/native_engine/resource_loader.rs:9008-9019` resolves the poster with `document_url.join(href)`. The `HTMLVideoElement.poster` getter similarly resolves with `new URLNative(value, baseUrl)` where `baseUrl` defaults to `host.url` (`crates/glass-browser/src/browser/native_engine/javascript.rs:43733,43766-43775`). No native-engine base-URL update/`<base href>` handling was found in the task-path implementation. With a page-level `<base href>`, both the reflected property and fetched resource can therefore point to a different URL than the Document's base-URL algorithm requires.

**Required before pass:** Resolve poster URLs through the owner Document's effective base URL for both the IDL property and the parent fetch. Add an integration case with `<base href>` pointing to a different path and assert both `video.poster` and the requested URL.

### [P2] [blocking] URL Basic authorization can be reattached after a cross-origin redirect

**Contract:** The poster request must use URL credentials (`docs/plan/tasks/native-engine-browser-874.md:45-49`; profile `docs/plan/native-engine-browser-profile.md:545-548`). The [Fetch Standard's HTTP-redirect fetch](https://fetch.spec.whatwg.org/#http-redirect-fetch) removes `Authorization` when a redirect crosses origins; it must not reappear later in that redirect chain.

**Evidence:** `crates/glass-browser/src/browser/native_engine/resource_loader.rs:6800-6806` derives `authorization_origin` and Basic credentials once from the original URL. For each hop, `:6882-6885` adds those credentials whenever the current hop's origin again equals that original origin. A credentialed URL on origin A redirected through origin B and back to A therefore reattaches the original `Authorization` header after it should have been stripped. The integration test verifies only initial URL-credential application (`crates/glass-browser/tests/native_engine.rs:74927-74933`), not redirect stripping.

**Required before pass:** Make authorization stripping persistent for the redirect chain once a cross-origin hop occurs, in line with Fetch redirect behavior. Add a loopback A-to-B-to-A redirect test asserting that the final A request has no `Authorization` header and that URL userinfo is not forwarded.

### [P3] [non-blocking] Stale in-flight response protection lacks a race regression

**Contract/evidence:** The task requires stale results not to replace a newer poster (`docs/plan/tasks/native-engine-browser-874.md:54-58`). The implementation has a source-identity guard in `crates/glass-browser/src/browser/native_engine/dom.rs:3306-3315`, and dynamic mutations refresh poster load/resource state before loading (`crates/glass-browser/src/browser/native_engine/content_process.rs:20718-20727`). The process-backed test covers initial load, same-value assignment without a duplicate request, replacement, removal, failure behavior, media state, and painting (`crates/glass-browser/tests/native_engine.rs:74998-75157`); a separate process-backed test checks CSP rejection before network access (`:75163-75203`). Neither test races an older in-flight response against a newer value/removal. This is a test gap, not a source-level mismatch found in the stale-result guard.

**Suggested follow-up:** Add a deterministic delayed-response test that mutates or removes `poster` before the first response completes and confirms the stale image never paints.

## Contract checks with no additional blocking finding

- **Parent-only cookie authority and IPC:** The child sends scoped owner/document metadata and cookie writes to the parent broker (`content_process.rs:1717-1733`). The parent supplies only image response data and the Document-scoped cookie projection (`:7237-7248`); the child bounds that projection before updating script-visible state (`:1752-1770`). Request-cookie selection and response-cookie storage occur in the parent resource loader. I found no path sending raw `Cookie`/`Set-Cookie` headers, the complete jar, or HttpOnly values into the content process. The failed-response cookie acceptance defect above is distinct from this authority boundary: the parent remains the authority but fails to apply some eligible response cookies.
- **Request metadata:** The broker validates the fixed `client=document`, `destination=image`, `initiator_type=video`, `credentials_mode=include`, and `use_url_credentials=true` values (`content_process.rs:9957-9979`). Per-hop cookies are selected by the parent (`resource_loader.rs:6897-6909`).
- **CSP/referrer and failure behavior:** Poster fetches use image policy and redirect checks (`resource_loader.rs:6945-6966`); process-side network/unsupported/limit failures are swallowed without failing the document (`content_process.rs:17161-17168`). CSP blocking is exercised by the process-backed test above. Referrer selection has the blocking per-video override noted above.
- **Lifecycle, duplicate loads, and paint:** Initial discovery and dynamic mutation both call the poster loader (`content_process.rs:16793-16800,20718-20727`). Source-keyed load tracking and stale-result checks are present (`dom.rs:3215-3268,3306-3315`). Poster painting is a distinct image command using a centered contain rectangle (`paint.rs:550-579,581-613`); poster dimensions and transfer limits are checked in `dom.rs:3271-3304`. The behavioral integration test is process-backed with loopback HTTP servers, not a mock/fake integration.
- **Load/event behavior:** The integration test verifies that a 404 leaves no painted poster, dispatches no video `load`/`error` events, and does not alter the media state (`tests/native_engine.rs:75117-75141`). Initial poster loading is awaited in the document resource-loading path (`content_process.rs:16793-16800`).

## Conclusion

**BLOCKED.** Four P2 contract mismatches remain: response cookies are dropped on failed poster responses; a video attribute overrides the Document referrer policy; poster URLs ignore the Document base URL; and URL Basic authorization can be restored after a cross-origin redirect. The parent-only cookie-authority boundary and process-backed integration are otherwise present. No Cargo command was run: source inspection established the contract mismatches, and no build/test invocation was needed to decide this review. No implementation files or remote state were changed.
