---
id: native-engine-browser-831
scope: glass-browser/native-engine/module-script-referrer-policy
status: complete
depends-on: [native-engine-browser-830]
---

# Glass native-engine browser slice 831: module-script referrer policy

## Objective

Apply the owning Document and `HTMLScriptElement.referrerPolicy` policy to
module-script entry fetches and their recursively fetched static dependencies,
including the policy inheritance rules carried by module responses.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-830.md`
- [Issue #40](https://github.com/wanazhar/glass/issues/40)
- [HTML Standard: `script` element](https://html.spec.whatwg.org/multipage/scripting.html#the-script-element)
- [HTML Standard: fetching scripts](https://html.spec.whatwg.org/multipage/webappapis.html#fetching-scripts)
- [HTML Standard: `HostLoadImportedModule`](https://html.spec.whatwg.org/multipage/webappapis.html#hostloadimportedmodule-referrer-modulerequest-loadstate-payload)
- [Referrer Policy: redirect processing](https://w3c.github.io/webappsec-referrer-policy/#integration-with-fetch)

## Contract

- For network-backed module scripts, recognize all eight standard Referrer
  Policy tokens ASCII-case-insensitively. An external module element's valid
  policy overrides the owning live Document's response/meta default for its
  entry fetch and static dependency graph. An inline module uses its element
  policy for dependencies. Missing, empty, and invalid attributes inherit the
  current owning Document policy.
- Compute the entry request's `Referer` from the Document URL. Compute each
  static dependency request's `Referer` from the referencing module's response
  URL (after redirects), under that module's effective fetch policy; do not
  substitute the owning Document URL for module-to-module requests.
- A recognized `Referrer-Policy` response header on a module response updates
  that module's fetch options for its own dependencies. An absent or
  unrecognized header leaves the inherited policy unchanged. Redirect response
  policies update only the next redirect hop. Preserve the response policy in
  reusable script-cache entries and apply 304 metadata updates without losing
  the cached representation's effective policy.
- Apply this to parser-discovered and dynamically inserted module elements and
  their recursive static imports. Preserve module URL resolution, import maps,
  module types, CORS/credentials, CSP, integrity, cookies, redirect and byte
  limits, cache identity/freshness, and execution/event behavior. Blob and
  rooted-file modules remain on their existing non-network paths.
- A process-backed two-origin HTTP regression verifies actual entry,
  dependency, inline-module, and dynamically inserted module `Referer` headers,
  per-response inheritance through multiple graph levels, and redirect
  updates. Pure tests cover cache policy preservation and 304 replacement
  semantics.
- Dynamic `import()` policy inherited from the active module, module workers,
  worklets, preload/modulepreload, broader Referrer Policy/Fetch or WPT
  conformance, remote CI, and cross-platform certification remain separate
  requirements; this slice must not claim complete module-fetch conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-831.md`

## Verification

- `cargo check -p glass-browser --features native-engine --lib --tests --locked -q`
  passed. Its warnings are the existing dead-code warnings in the superseded
  HTML parser; the module-policy changes add no warning.
- `cargo test -p glass-browser --lib --features native-engine referrer --locked --quiet`
  passed (9 passed, 1,637 filtered; 8.01 seconds), including the module source
  policy and cache/304 metadata tests.
- The process-backed two-origin regression passed in the focused
  `native_content_process_inherits_referrer_policy_through` test batch. Its
  static-module case verified the actual entry and recursive dependency
  `Referer` headers. The dynamic-module settlement failure remains tracked in
  Slice 832. Slice 833's previously failing module-worker regression now passes
  locally and remains in progress only because it depends on Slice 832.
- `cargo fmt --all` and `git diff --check` passed.
- Maintainer documentation gates passed: release-documentation truth (1,459
  Markdown documents, zero current-claim failures), documentation depth (93
  routed guides and 19 substantive contracts), TUI shortcut inventory (15
  implementation keys and 63 documentation markers), and documentation
  coverage (1,459 Markdown files, 346 MCP tools, 17 examples, 22 public
  modules).
- Remote CI, WPT conformance, and cross-platform certification are not part of
  local results and must not be claimed without evidence.
