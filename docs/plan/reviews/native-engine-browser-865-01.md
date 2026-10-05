# Native engine Slice 865 review

- **Task:** `native-engine-browser-865`
- **Review mode:** direct self-review; no independent agent review was used.

## Findings

None. The browser-owned SharedWorker coordinator and the content-process page
EventSource route both use the parent loader for cookie matching and response
cookie processing; the regression confirms the CORS-error result is consistent
across these two owners.

## Review notes

- The process-backed two-origin test verifies the coordinator's actual
  credentialed EventSource GET carries the parent-selected HttpOnly page seed
  and SharedWorker-entry cookies, includes the page/worker Origin, and has no
  preflight.
- The actual response combines wildcard `Access-Control-Allow-Origin`,
  `Access-Control-Allow-Credentials: true`, an HttpOnly `Set-Cookie`, and SSE
  data. Credentialed CORS rejects the response; the SharedWorker observes an
  error without open/message data and closes the source.
- The parent cookie API retains the actual-response cookie beside the seed
  and worker-entry cookies. A later authorized page Fetch sends all three;
  `document.cookie` remains empty. No cookie profile, raw header, or complete
  jar is transferred to the SharedWorker.
- No production code change was needed: the parent-loader ordering fixed in
  Slice 864 is shared by the browser-owned coordinator. This is focused local
  evidence, not EventSource redirect/reconnect, complete Fetch/CORS or WPT,
  cross-platform, independent, or remote-CI certification.
- Standards reference: [Fetch Standard, CORS protocol and credentials](https://fetch.spec.whatwg.org/#cors-protocol-and-credentials).

## Verification

- Scoped `cargo check -p glass-browser --features native-engine --lib --test
  native_engine --locked --quiet` passed with successful compiler output
  suppressed.
- Exact `native_runtime_shared_worker_event_source_cors_errors_keep_parent_cookies`
  passed (1 passed; 928 filtered; 22.94 seconds).
- The release-documentation, documentation-depth, TUI-shortcut, and
  documentation-coverage gates passed after the final documentation edits.
  Coverage used the existing shared-target `glass` and `glass-browser`
  binaries by explicit path. `rustfmt --check` and `git diff --check` passed.

## Conclusion

**Pass (direct self-review; not an independent review).**
