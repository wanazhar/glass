# Slice 873 direct self-review

Review mode: direct author self-review. No independent reviewer or agent was
used.

## Scope reviewed

- Parent/content-process boundary for HTML `as=image` preloads.
- Initial discovery, duplicate URLs, dynamic insertion and `href` replacement.
- Matching document-local image reuse and decoded-pixel/cache bounds.
- Cookie projection and response-cookie ownership across preload requests.
- Public capability and architecture wording versus implemented scope.

## Findings

- Every HTTP(S) preload uses the existing owner-checked parent image broker.
  Parent-side CSP, request-cookie selection, redirects and response-cookie
  processing remain authoritative; the child receives pixels and its
  URL-scoped script-visible cookie projection, never cookie headers, the jar,
  or HttpOnly values.
- Preload reuse is document-local, keyed by resolved URL, excludes the URL
  fragment, and is bounded to 2 MiB decoded pixels and 64 image resources.
  `no-store` responses can still satisfy the matching document image without
  changing ordinary HTTP-cache policy.
- Initial links run before ordinary image requests. Dynamic links and
  `href` changes are processed through the owning content turn. Unsupported
  metadata stays inactive, and successful/failed attempts use existing
  bounded `load`/`error` events.
- The implementation deliberately does not claim responsive selection,
  complete destination/CORS/credential/integrity matching, non-image
  preloads, `modulepreload`, fetch-priority scheduling, or WPT/browser-wide
  conformance. The architecture matrix retains those gaps.
- One implementation edge to keep visible: if the decoded cache budget is
  exhausted, the preload may succeed and dispatch `load`, but a later image
  cannot reuse it and follows the normal parent request path.

## Verification

- Exact process-backed regression passed (1 passed; 934 filtered; 24.81
  seconds), including duplicate discovery, parent-owned HttpOnly cookie
  reuse, visible-cookie filtering, unsupported CORS metadata, dynamic load and
  error events, and matching image reuse despite `no-store`.
- Eleven focused preload unit tests and the exact capability-profile test
  passed. The scoped package check completed; only pre-existing dead-code
  warnings were emitted.
- Rust formatting and `git diff --check` passed on the final code/docs tree.
- Release-documentation audit passed: 1,529 Markdown documents, zero
  current-claim failures. Documentation depth passed: 93 guides and 19
  contracts. Shortcut inventory passed: 15 implementation keys and 63
  markers. Documentation coverage passed: 1,529 Markdown files, 346
  full-product MCP tools (101 browser-only), 17 examples, and 22 public
  modules.

## Conclusion

The slice adds bounded image preloading without transferring network or cookie
authority to the content process. It closes only this narrow feature gap;
Issue #40 remains open, with broader native-browser parity and platform gates
still outstanding.
