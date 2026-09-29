---
id: native-engine-browser-829
scope: glass-browser/native-engine/classic-script-referrer-policy
status: done
depends-on: [native-engine-browser-828]
---

# Glass native-engine browser slice 829: classic script referrer policy

## Objective

Apply the owning Document and `HTMLScriptElement.referrerPolicy` policy to
external classic-script requests initiated by parser-discovered and dynamically
inserted HTML script elements.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-828.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [HTML Standard: `script` element](https://html.spec.whatwg.org/multipage/scripting.html#the-script-element)
- [HTML Standard: referrer-policy attributes](https://html.spec.whatwg.org/multipage/urls-and-fetching.html#referrer-policy-attributes)
- [Referrer Policy: redirect processing](https://w3c.github.io/webappsec-referrer-policy/#integration-with-fetch)

## Contract

- For an external classic script, recognize all eight standard Referrer Policy
  tokens ASCII-case-insensitively. The script element's effective policy is
  captured for that fetch. Missing, empty, and invalid attributes use the
  owning live Document's current response-header or meta-derived policy.
- `HTMLScriptElement.referrerPolicy` reflects the `referrerpolicy` content
  attribute as a limited-known-values string. Invalid attribute text reads as
  the empty string; setting the property updates the attribute and a subsequent
  script fetch observes it. Changing it after dispatch does not rewrite an
  already-started request.
- The policy applies to the initial network fetch of external classic scripts,
  both parser-discovered and dynamically inserted. This slice does not change
  inline-script behavior, module-script entry/graph policy, worker requests, or
  other request initiators.
- Compute each outgoing `Referer` from the original owning Document URL under
  the effective policy, using the existing Fetch helper. Strip fragments and
  credentials according to that helper. A recognized `Referrer-Policy` header
  on a redirect response updates the policy for the next hop; unknown or absent
  values leave it unchanged.
- Preserve existing script URL resolution, CSP, mixed-content, CORS, cookies,
  MIME/integrity checks, byte/redirect limits, script cache identity, freshness,
  and execution/event behavior. A cache hit sends no request; a conditional
  revalidation sends the current request's computed Referer.
- A process-backed local HTTP regression checks actual same-origin and
  cross-origin headers, Document response-header fallback, live meta fallback
  for a dynamic script, valid/invalid reflection and IDL mutation, a redirect
  policy update, and script-cache revalidation.
- This slice does not establish complete Referrer Policy/Fetch or WPT
  conformance, platform certification, or browser-completion gates.

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-829.md`

## Verification

- `cargo check -p glass-browser --features native-engine --test native_engine --locked --quiet` was run before the focused test and found one moved-value error in the test fixture; cloning the expected referrer fixed it. The focused test then compiled the `native_engine` integration target and passed, so no redundant metadata-only recheck was run.
- `cargo test -p glass-browser --features native-engine --test native_engine native_content_process_applies_classic_script_referrer_policy --locked --quiet -- --exact --nocapture` passed (1 passed, 877 filtered; 33.70 seconds). Actual local HTTP requests verified same-origin/cross-origin headers, Document response policy fallback, empty/invalid fallback, case-insensitive `ORIGIN`, no-referrer, redirect policy updates, dynamic IDL mutation, live meta fallback, ETag/304 revalidation with the current policy, and execution of all fetched scripts.
- `cargo fmt --all -- --check` passed after applying `cargo fmt --all` to this slice.
- `git diff --check` passed.
- Remote CI, WPT conformance, and cross-platform certification are not part of
  this slice and must not be claimed from local test results.
