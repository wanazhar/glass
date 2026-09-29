---
id: native-engine-browser-827
scope: glass-browser/native-engine/meta-referrer-document-policy
status: complete
depends-on: [native-engine-browser-826]
---

# Glass native-engine browser slice 827: live meta referrer policy

## Objective

Implement HTML `meta name="referrer"` as a live update to the owning
Document's referrer-policy default used by page Fetch.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-826.md`
- `docs/maintainers/README.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [HTML Standard: meta referrer processing](https://html.spec.whatwg.org/multipage/semantics.html#meta-referrer)
- [Fetch Standard: main fetch](https://fetch.spec.whatwg.org/#main-fetch)

## Contract

- Referrer policy is scoped to each live Document's native DOM state and page
  bridge. The HTTP response policy established by slice 826 seeds a new
  Document; meta updates are not written into URL-keyed shared loader state.
  A different live Document must not observe this Document's meta policy
  merely because their URLs match.
- When a `meta` element enters the Document tree, or its `name` or `content`
  attribute changes while connected, apply the HTML processing algorithm:
  match `name="referrer"` ASCII-case-insensitively; require a present,
  non-empty `content`; convert content to ASCII lowercase without trimming;
  translate `never`, `always`, `origin-when-crossorigin`, and `default` to
  their specified policies; and ignore values that do not identify a policy.
- A valid update takes effect immediately in event order, regardless of tree
  order. Removal does not restore an earlier policy. Invalid, empty, or
  unrelated updates do not change the current policy. The parser path applies
  source meta elements, and inserting a subtree applies its connected meta
  descendants in insertion order.
- Page Fetch with an empty request-level policy uses the current Document
  policy before Service Worker handoff and network dispatch. Explicit request
  policies keep precedence, and a constructed `Request.referrerPolicy`
  remains empty when only the Document default supplies the effective value.
- A process-backed two-origin test covers the parsed response plus live
  insertion, attribute edits, alias handling, tree-order independence,
  removal persistence, and actual outgoing `Referer` values.
- This slice does not implement element-level `referrerpolicy`/`noreferrer`,
  independent Worker policy containers, inheritance, non-Fetch request
  initiators, or broad Referrer Policy/WPT conformance. It does not claim
  browser completion.

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-827.md`

## Verification

- `cargo fmt --all -- --check` passed.
- `cargo check -p glass-browser --features native-engine --test native_engine --locked --quiet` passed with 68 existing dead-code warnings from the superseded HTML parser.
- `cargo test -p glass-browser --features native-engine --test native_engine native_content_process_inherits_document_referrer_policy_for_page_fetch --locked -- --exact --nocapture` passed (1 passed, 875 filtered; 52.01 seconds). It covers the response-header seed plus parsed and live meta updates, actual cross-origin `Referer` values, and 17 target requests.
- `git diff --check` passed.
- Remote CI and cross-platform certification were not run or claimed.
