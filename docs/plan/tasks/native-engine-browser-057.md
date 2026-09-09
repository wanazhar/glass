---
id: native-engine-browser-057
scope: glass-browser/native-engine/fetch-urlsearchparams
status: done
depends-on: [native-engine-browser-056]
---

# BE-03ab/BE-04am: bounded URLSearchParams fetch bodies

## Objective

Support the common URL-encoded API/form request path through the existing
fetch/XHR owner, keeping encoding and network policy deterministic.

## Contract

- The page realm exposes bounded string-only `URLSearchParams` construction,
  append/set/delete/get/getAll/has, `entries`, and `toString` helpers.
- `fetch()` and XHR `send()` accept URLSearchParams for POST and serialize
  `application/x-www-form-urlencoded;charset=UTF-8` bodies with bounded
  percent encoding and `+` space encoding.
- The generated content type and body remain subject to the existing request
  size, CSP, mixed-content, cookie, referrer, redirect, CORS, and preflight
  policy.

## Deliberate boundary and tradeoffs

Object/record/sequence constructors, sorting, full iterator/Web IDL identity,
streaming, and browser URL/search-parameter integration remain open. The
surface intentionally supports only string-oriented bounded API payloads.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `native_content_process_fetches_bounded_url_search_params` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine fetch -- --nocapture` — 10 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.
