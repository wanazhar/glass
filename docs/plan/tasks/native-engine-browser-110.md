---
id: native-engine-browser-110
scope: glass-browser/native-engine/fetch-redirects
status: done
depends-on: [native-engine-browser-109]
---

# BE-29: bounded Fetch redirect modes

## Objective

Expose the common Fetch redirect modes through the existing request and
response owners while preserving bounded redirect validation and method
rewriting.

## Contract

- Fetch defaults to `redirect: "follow"` and accepts `follow`, `error`, and
  `manual`; unsupported values fail as a JavaScript `TypeError` before network
  dispatch.
- `follow` retains the bounded redirect chain and existing URL, CSP,
  mixed-content, credential, referrer, cookie, CORS, and request-size checks.
  A response that followed at least one hop exposes `redirected: true` and its
  final URL. Existing 301/302/303 method rewriting and 307/308 body retention
  remain the single request owner.
- `error` rejects at the first HTTP redirect without requesting its location.
- `manual` returns a filtered `opaqueredirect` response with `type` set to
  `opaqueredirect`, status `0`, `ok: false`, an empty URL/header view, and
  rejected body reads. Redirect location headers and raw response data are not
  transferred into the script realm.
- `same-origin` also rejects a redirect whose next URL has a different origin.
  A `no-cors` request remains opaque if any followed hop crosses origin, even
  if the final URL returns to the document origin.
- Service-worker redirect routing, complete redirect-status/referrer parity,
  streaming bodies, private-network access, and complete Fetch/Response Web IDL
  parity remain open.

## Ownership and sequence

```text
Fetch options -> redirect mode validation -> typed content-process request
  -> bounded redirect loop -> basic/opaque/opaqueredirect response projection
```

The existing resource loader remains the only redirect and network owner. The
content process transfers only bounded response metadata; JavaScript owns the
filtered Response shell.

## Deliberate boundary and tradeoffs

The implementation covers the high-value redirect choices without exposing
`Location` or unbounded redirect state to page scripts. Manual redirects are
intentionally opaque, and the existing fixed hop/size/security budgets remain
authoritative. Full Fetch redirect, service-worker, streaming, and Web IDL
parity are outside this slice.

## Paths

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

The slice was validated with the affected native-engine integration target.
The issue-level full native-engine suite, strict-Clippy baseline,
documentation, release-truth, remote-CI, publication, and browser-parity
gates remain final issue gates; this task makes no remote or release claim.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_fetches_bounded_redirect_modes -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_fetches_ -- --nocapture` — 6 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_opaque_no_cors_response -- --nocapture` — 1 passed
- `python3 scripts/check-documentation-coverage.py` — 760 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  760 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=934; current-claim failures=0
