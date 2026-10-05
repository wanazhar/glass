# Native engine Slice 848 review

- **Revision reviewed:** Slice 848 worktree diff atop `59b50d1a`
- **Task:** `native-engine-browser-848`
- **Review mode:** direct self-review; no independent agent review was used.

## Findings

None. No blocking contract mismatch was found.

## Review notes

- Script-created page and frame nodes rebind event-listener and handler keys
  when their temporary IDs become native IDs. Their event-owner getters follow
  the assigned node ID, so image/media events resolve to the registered
  callbacks (`javascript.rs:43022-43053`, `45193`, `53406-53412`, `53927`,
  `54741`).
- Mutation requests now carry the exact committed context, frame, generation,
  and document URL. The parent still verifies equality with each brokered
  script owner and checks it against the active document; no owner validation
  was relaxed (`content_process.rs:4823-4877`, `6637-6665`).
- Cookie ownership remains in the parent. It applies validated script-visible
  writes to the parent loader, selects request cookies, accepts response
  cookies, and updates only the URL-scoped `document.cookie` projection. The
  process-backed regression verifies HttpOnly cookie progression and that the
  page cannot read those values (`content_process.rs:1110-1160`,
  `6670-6760`; `tests/native_engine.rs:13825-14115`).
- Image and media loaders now recognize an unchanged source as already
  attempted, including failed loads. This prevents a CSP-denied image from
  redispatching `error` while callback Fetches settle; a changed selected
  source remains eligible for a new attempt (`dom.rs:2843-2848`,
  `3091-3099`; `content_process.rs:16836-16989`).
- The focused process-backed regressions pass for the stylesheet case and the
  image/media case (2 passed). They verify callback Fetch order, the blocked
  URL never reaching transport, and parent-owned HttpOnly cookie rotation.
  This is local evidence only; no remote CI, WPT, or cross-platform result is
  claimed.

## Conclusion

**Pass (direct self-review; not an independent review).**
