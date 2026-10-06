# Independent Follow-up Review: Native Engine Browser Slice 874

- Task: `docs/plan/tasks/native-engine-browser-874.md`
- Base commit: `b327f6e28bc63dcbeb729bbca575b625302a6974`
- Reviewed checkout: `/home/ubuntu/work/glass/.worktrees/native-engine-browser-874`
- Scope: current uncommitted changes in the poster loader, DOM and JavaScript URL handling, content-process broker, and `tests/native_engine.rs`; prior findings in `native-engine-browser-874-01.md`.

## Prior blocking findings rechecked

### [P2] [non-blocking] Resolved: eligible poster response cookies survive failed loads

`crates/glass-browser/src/browser/native_engine/resource_loader.rs:6917-6926` now processes each response's `Set-Cookie` headers immediately after receiving the response, before redirect validation, final status checks, MIME and size checks, body reads, or image decoding. This keeps cookie acceptance independent of whether the poster image can be used.

The process-backed case in `crates/glass-browser/tests/native_engine.rs:74968-75008` returns HttpOnly cookies on a 404, undecodable image, and redirect to an unsupported scheme, then asserts all three cookies are sent on `/after-failure.png`. The test also checks the parent cookie API and that `document.cookie` contains only the non-HttpOnly owner cookie (`:75218-75241`). These assertions meaningfully cover acceptance and reuse on failed poster responses.

### [P2] [non-blocking] Resolved: poster requests use the Document referrer policy

`crates/glass-browser/src/browser/native_engine/content_process.rs:17149-17159` now passes `document.document_referrer_policy()` to the parent broker. The old video-element policy accessor has been removed from `crates/glass-browser/src/browser/native_engine/dom.rs`.

`native_content_process_video_poster_uses_document_referrer_policy` sets a restrictive response `Referrer-Policy: no-referrer` and a conflicting `referrerpolicy='unsafe-url'` on the video, then inspects the actual poster request and asserts it has no `Referer` header (`crates/glass-browser/tests/native_engine.rs:75266-75319`). This directly distinguishes Document policy from the video attribute.

### [P2] [non-blocking] Resolved: poster reflection and fetching use the effective base URL

`NativeDocument::effective_base_url` finds the attached HTML `<base href>` and resolves it against the document URL (`crates/glass-browser/src/browser/native_engine/dom.rs:5070-5090`). The video poster getter resolves against the live effective base (`crates/glass-browser/src/browser/native_engine/javascript.rs:43718-43737, 43788-43798`), while `load_external_video_posters` resolves the fetch target from that same document base (`crates/glass-browser/src/browser/native_engine/content_process.rs:17126-17159`). Base changes clear poster load state before refresh (`:20731-20743`).

`native_content_process_video_poster_resolves_against_document_base_url` checks the initial reflected URL and `/assets/cover.png` request, then changes and removes the base and checks both the resulting reflected URLs and the `/changed/cover.png` and `/cover.png` requests (`crates/glass-browser/tests/native_engine.rs:75323-75411`).

### [P2] [non-blocking] Resolved: URL Basic authorization stays stripped after a cross-origin redirect

The loader records whether any redirect crosses origins and suppresses the original URL credentials thereafter (`crates/glass-browser/src/browser/native_engine/resource_loader.rs:6800-6807, 6880-6888, 6963-6975`).

`native_content_process_video_poster_url_credentials_stay_stripped_after_cross_origin_redirect` exercises origin A → B → A. It asserts the initial A request has Basic authorization, B has none, and the final A request still has none; it also checks userinfo is absent from the requests (`crates/glass-browser/tests/native_engine.rs:75414-75498`). This covers the A-to-B-to-A regression from the prior report.

## Cookie authority and IPC boundary

The parent remains the only request-cookie selector and response-cookie store: request cookies are attached in `resource_loader.rs:6899-6910`, and response `Set-Cookie` is applied there at `:6917-6926`. The content-process request carries the owner, DOM cookie-write journal, and fixed image metadata, not a cookie jar or request/response cookie headers (`content_process.rs:1717-1732`). The parent response carries the decoded image and a document-scoped cookie projection (`:7237-7248`); the child bounds and installs only that projection (`:1752-1770`). Nonempty cookie-change journals returned by the content process are rejected (`:5135-5149`).

In the reviewed poster path, I found no transfer of the cookie jar, HttpOnly cookie values, or raw `Cookie`/`Set-Cookie` headers over IPC. The cookie test additionally demonstrates parent-side HttpOnly cookie reuse while keeping those values out of `document.cookie` (`tests/native_engine.rs:74990-75000, 75218-75241`).

## Findings

### [P3] [non-blocking] No delayed-response regression for stale poster completion

The existing source guard refuses to install a result when the current poster attribute or load marker no longer matches (`crates/glass-browser/src/browser/native_engine/dom.rs:3311-3318`). The five poster process-backed tests cover cookies, referrer policy, base URL updates, redirect credentials, and CSP, but none delays an older poster response while the poster or base changes or is removed. The sequential base URL test does not exercise that race. This remains a non-blocking test gap; the source guard is present and I found no additional blocking mismatch in this review.

## Verification status

A separate validator reported that the scoped Cargo check passed and that rustfmt and `git diff --check` passed. The same validator reported all five process-backed poster tests failed before engine startup at `TcpListener::bind("127.0.0.1:0")` with `EPERM` in the managed sandbox. The affected tests are the five poster tests listed above in `tests/native_engine.rs` (including `native_content_process_blocks_csp_disallowed_video_poster_before_request` at `:75502-75538`). This is environment-blocked verification: it is neither a product assertion failure nor a test success. No Cargo command was run during this independent review.

## Conclusion

**BLOCKED.** Static review finds the four prior P2 contract mismatches resolved, with meaningful process-backed regressions added, and finds no remaining blocking source mismatch. The P3 stale-response race test gap is non-blocking. The process-backed regressions could not be verified because loopback listener creation was denied by the managed sandbox, so this review cannot conclude PASS.
