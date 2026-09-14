# Native cookie policy and metadata (334)

Status: implemented locally in `1253efc7` (`feat(native-engine): enforce
cookie site policy`).

This slice completes the cookie metadata and request-context boundary used by
the native loader:

- SameSite (`Strict`, `Lax`, and `None`) and eviction priority (`Low`,
  `Medium`, and `High`) survive profile import/export and `Set-Cookie`
  parsing;
- `SameSite=None` is rejected unless the cookie is Secure;
- cookies without an explicit SameSite value use the browser-compatible Lax
  default;
- top-level safe navigations, Fetch/XHR, EventSource, WebSocket, and document
  subresources apply schemeful same-site filtering with the current initiator;
- redirects and request classes retain the existing shared URL, CORS, CSP,
  mixed-content, and cookie owners; and
- invalid attributes fail closed instead of entering durable profile state.

The native loader uses a bounded registrable-site approximation for ordinary
hostnames. A full public-suffix-list owner and partitioned-cookie model remain
separate profile work; this slice does not claim those semantics.

## Local evidence

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --lib cookie_ --locked` — 7 passed
- `cargo clippy --quiet -p glass-browser --lib --tests --locked -- -D warnings`

All evidence is local; this checkout has not been pushed and has no remote CI
result.
