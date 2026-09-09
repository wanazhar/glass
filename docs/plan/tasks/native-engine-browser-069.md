---
id: native-engine-browser-069
scope: glass-browser/native-engine/document-cookie-session-synchronization
status: done
depends-on: [native-engine-browser-068]
---

# BE-02/BE-03/BE-04: bounded `document.cookie` session synchronization

## Objective

Connect the existing Rust-owned HTTP session cookie jar to the shared
QuickJS document host so network pages can read visible cookies, set bounded
cookie lines, and observe those changes on later navigation/fetch requests.

## Contract

- Network page realms expose a bounded `document.cookie` getter containing
  matching non-HttpOnly cookies in the existing deterministic header order.
- The setter accepts a bounded cookie line and queues it as a typed host
  command; Rust applies the existing domain, path, Secure, Max-Age, and size
  policy before the next transport request.
- HTTP `Set-Cookie` response values are available to the next page evaluation;
  HttpOnly cookies remain in request headers but are filtered from the page
  getter.
- Child-owned HTTP navigation and fetches use the same jar for page reads,
  page writes, navigation headers, and credentialed fetch headers.
- Non-network/opaque documents expose an empty cookie view and cannot create a
  transport cookie jar entry.

## Deliberate boundary and tradeoffs

Cookies remain process-owned session state. Cookie profile persistence,
SameSite/partitioned-cookie policy, full Expires/date parsing, cookie-change
events, third-party policy, and full Cookie/Document Web IDL identity remain
open. A setter's optimistic same-evaluation getter is bounded host behavior;
the Rust jar is authoritative for later evaluations and requests.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo build --quiet -p glass-browser --features native-engine --bin glass-native-content-worker` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_synchronizes_document_cookie_with_http_session -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine cookie_state_obeys_scope_security_expiration_and_bounds -- --nocapture` — 1 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.
