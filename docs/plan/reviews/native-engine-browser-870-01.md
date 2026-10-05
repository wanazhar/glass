# Native engine Slice 870 review

- **Task:** `native-engine-browser-870`
- **Review mode:** direct self-review; no independent agent review was used.

## Findings

No blocking defect found in the browser-owned SharedWorker WebSocket path. The
coordinator distinguishes an HTTP response error from transport failures,
applies eligible `Set-Cookie` values to its parent-owned shared-context
loader, and only then dispatches the normal error and abnormal-close events.
The error text contains the HTTP status, not response headers or the body.
Timeouts and errors without HTTP responses retain their existing paths.

The process-backed regression exercises this coordinator independently from
Slice 869's page IPC broker. It confirms parent-selected handshake cookies,
response-cookie acceptance and later reuse, HttpOnly filtering, and the
absence of open, retry, or redirect behavior. Cookie matching, acceptance, and
storage remain in the parent; no raw cookie data is sent to the worker.

## Verification

- Scoped `cargo check -p glass-browser --test native_engine --locked` passed
  using `/home/ubuntu/work/glass/target`, with successful compiler output
  suppressed.
- Exact
  `native_runtime_shared_worker_websocket_failed_handshake_keeps_parent_cookies`
  passed.
- Rust formatting, `git diff --check`, and all four maintainer documentation
  gates passed after the final documentation edits.
- No remote CI or cross-platform certification is claimed.

## Conclusion

**Pass (direct self-review; not an independent review).**
