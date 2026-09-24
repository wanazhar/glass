---
id: native-engine-browser-722
scope: glass-browser/security/rooted-file-script-csp
status: completed
depends-on: [native-engine-browser-718, native-engine-browser-721]
---

# Glass native-engine browser slice 722: rooted-file script CSP

## Objective

Install parser-sourced Content Security Policy meta declarations for
configured-root file Documents before script execution, and enforce their
script-source policy for inline scripts and every local script/module fetch.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-718.md`
- `docs/plan/tasks/native-engine-browser-721.md`
- [HTML Standard: CSP meta pragma](https://html.spec.whatwg.org/multipage/semantics.html#attr-meta-http-equiv-content-security-policy)
- [Content Security Policy Level 3: policy and self-origin](https://www.w3.org/TR/CSP/#framework)
- [Content Security Policy Level 3: source matching](https://www.w3.org/TR/CSP/#match-url-to-source-expression)
- [URL Standard: file origins](https://url.spec.whatwg.org/#origin)
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- Read bounded parser-sourced `<meta http-equiv="Content-Security-Policy">`
  declarations from the Document head and install them before local initial
  scripts, stylesheets, images, media, or font resources are loaded. Preserve
  policy conjunction and directive fallback semantics already implemented by
  the shared CSP parser.
- For this configured-root file profile, `'self'` matches script resources
  admitted by the same most-specific configured file root as the Document.
  This gives file Documents a stable policy boundary despite platform-defined
  file-origin behavior. An explicit `file:` scheme source may match another
  root-admitted file resource; it never bypasses root admission or allows
  network transport.
- Enforce `script-src-elem`, `script-src`, and `default-src` fallback for
  inline classic/module scripts, external classic/module roots, static module
  dependencies, and invoked dynamic imports. Preserve nonce and existing
  script-source handling. A denied resource must not be read or evaluated and
  must settle through its existing script error or import-promise path.
- Keep CSP policy state document-keyed and replace parser policies on a later
  navigation to the same URL. Multiple policies remain conjunctive; no
  policy may relax another policy or the configured file-root boundary.
- Do not change HTTP(S) CSP behavior or introduce network transport for file
  Documents.
- This slice covers enforced parser-sourced script policy only. Dynamic CSP
  meta insertion, report-only file policies, other file subresource classes,
  complete CSP grammar/reporting, and complete file-origin behavior remain
  separate issue #40 work.

## Path

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-722.md`

## Verification

- Add process-backed rooted-file tests for inline blocking under
  `script-src 'none'`, same-root external/module/dynamic loading under
  `script-src 'self'`, and rejection of a script in a different configured
  root. Assert denied scripts are not evaluated. A loader-unit test uses an
  invalid-UTF-8 denied file to prove CSP rejects it before bytes are read.
- Preserve the rooted-file import-map, dynamic-import JSON, and file-boundary
  regressions, plus the network CSP script-policy regression set.
- Run one scoped `glass-browser` check before focused unit/integration tests;
  after the behavioral batch, run formatting, whitespace, and the maintainer
  documentation gates. Do not run workspace-wide or remote CI for this slice.

## Results

Parser-sourced CSP meta policies are installed for rooted-file Documents
before local resources load. Inline classic scripts, external classic/module
roots, static module dependencies, and invoked dynamic imports now enforce
`script-src-elem`/`script-src`/`default-src` fallback, nonce behavior, and the
configured-root boundary. In this file profile, `'self'` means the same
most-specific configured root; explicit `file:` sources remain constrained by
root admission. Same-URL navigation replaces parser meta policy, and denied
script bytes are not read.

`cargo check -p glass-browser --lib --test native_engine --locked --quiet`
passed. The focused rooted-file CSP integration test passed. The CSP unit
batch passed 16/16 tests; the CSP integration batch passed 18/19. The rooted-
file import-map, runtime dynamic-JSON, and unrooted-module regressions each
passed independently. The one failure,
`native_content_process_delivers_report_only_csp_report_uri_network_reports`,
timed out waiting for the report listener in both the batch and an exact
standalone replay; it covers report-only network delivery, not this slice's
enforced rooted-file script policy. The other CSP integration tests passed.
`cargo fmt --all -- --check`, `git diff --check`, and maintainer documentation
gates passed locally. Remote CI was not run; this branch remains local-only.
Other file subresource classes, report-only file policy, dynamic CSP meta
insertion, full CSP grammar/reporting, and complete file-origin behavior remain
open issue #40 work.
