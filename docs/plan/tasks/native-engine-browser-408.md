# Native CSP redirect path matching (408)

status: done
scope: native-engine/csp-redirect-path-matching
issue: 40

## Objective

Make the native engine apply CSP host-source paths at the initial request and
ignore only those paths on HTTP redirect hops, as required by CSP. Every
native subresource owner and embedded-frame navigation must share this rule;
scheme, host, port, mixed-content, credential, and URL-scheme boundaries must
remain enforced.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-404.md`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- [CSP Level 3](https://www.w3.org/TR/CSP/)

The 404 matcher correctly enforced source-expression paths, but applied the
same path comparison to every URL in a manually followed redirect chain. CSP
does not authorize a redirect by exposing the source path to the redirect
target: a redirect hop ignores the source-expression path while retaining the
other source-expression checks. The same distinction must hold for report-only
decisions so enforcement and observation do not disagree.

## Contract

- Initial Fetch, EventSource, stylesheet, image, script, and worker requests
  remain path-aware and use the complete shared CSP source matcher.
- Each manually followed HTTP redirect uses the same matcher with only the
  host-source path component ignored. Scheme, host, port, source upgrades,
  mixed-content rejection, credential rejection, URL validation, and resource
  limits remain unchanged.
- Redirected scripts preserve parser-inserted, nonce, and `strict-dynamic`
  semantics; ignoring a URL path cannot create or remove script trust.
- Report-only URL checks use the redirect-aware matcher on redirect hops and
  therefore do not report a path-only mismatch that enforcement correctly
  permits. Report-only state remains observational and never authorizes a
  blocked target.
- Embedded frames check their requested URL before loading and check the final
  URL with redirect-aware path semantics after HTTP loading. Page-navigation
  handoffs and non-redirect navigations remain path-aware as separate
  navigations.
- All policy lists continue to intersect, and all consumers retain one Rust
  matcher. No CDP fallback, path-stripping URL rewrite, or second policy owner
  is introduced.

## Non-goals

This slice does not add the remaining CSP grammar, `navigate-to` policy,
report-only meta policy, Subresource Integrity, or browser-wide CSP
conformance beyond the existing bounded source-expression grammar. Those
remain separate issue #40 gates.

## Implementation path

- Add explicit shared matcher and policy operations for redirect URL checks;
  keep the existing path-aware APIs as the initial-request default.
- Route all manually followed native resource redirects and their report-only
  records through the redirect operations.
- Retain the requested URL alongside content-process navigation results so an
  embedded frame can distinguish an HTTP redirect from a later page-script
  navigation handoff before checking the final URL.
- Add unit coverage for path-only redirect authorization and host preservation,
  plus native HTTP integration witnesses for redirected scripts and frames.
- Update the architecture, active plan, analysis, and this task with exact
  evidence.

## Tradeoffs

- Explicit redirect operations make the CSP exception visible at call sites
  and prevent accidental path stripping on ordinary requests, at the cost of
  a small amount of policy API surface.
- Carrying the last requested frame URL through the content-process handoff
  avoids adding redirect state to every document snapshot, while preserving
  the distinction between a network redirect and a user/page navigation.
- The native loader still follows bounded redirect chains itself. This keeps
  the change focused and preserves current cookie, referrer, CORS, cache, and
  mixed-content behavior, but does not claim complete redirect parity outside
  the documented CSP path rule.

## Delivered

- Added redirect-aware source and script matchers that ignore only CSP
  host-source paths while preserving scheme, host, port, and script trust
  checks.
- Updated enforced and report-only redirect decisions for Fetch, EventSource,
  stylesheets, images, scripts, and workers.
- Updated embedded-frame final URL checks to distinguish HTTP redirects from
  later page-navigation handoffs; the requested frame URL remains checked with
  its declared path before network loading.
- Added a unit witness for path-only redirect authorization and host
  preservation, an external-script redirect witness with no false report-only
  violation, and an embedded-frame redirect witness that loads the final
  document instead of falling back to `about:blank`.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --lib csp_source_expressions_match_scheme_host_port_path --locked -- --nocapture`
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --test native_engine ignores_csp_path_on --locked -- --nocapture`
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --test native_engine csp --locked -- --nocapture` (15 passed)
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked --no-fail-fast` (1,124 passed; 1 ignored)
- `python3 scripts/check-release-documentation.py --previous-version 0.3.13 --require-previous-version` (1,058 Markdown documents; 0 current-claim failures)
- `python3 scripts/check-documentation-depth.py` (93 current guides; 19 substantive contracts)
- `python3 scripts/check-tui-shortcuts.py` (15 implementation keys; 63 documentation markers)
- `python3 scripts/check-documentation-coverage.py --glass target/debug/glass --glass-browser target/debug/glass-browser` (1,058 Markdown files; 346 full-product MCP tools; 17 examples; 22 public modules)

The focused matcher test passed 1/1, and the redirect integration filter
passed 2/2 for script and embedded-frame redirects. The post-test process
audit found no Glass, Cargo, Rust, Chromium, or debug-port workers left
running.
