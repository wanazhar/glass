# Native CSP source-expression matching (404)

status: done
scope: native-engine/csp-source-expression-grammar
issue: 40

## Objective

Replace the native engine's exact-origin CSP shortcut with a bounded
source-expression matcher for the network and frame owners. The matcher must
make the same authorization and report-only decisions for valid scheme
sources and host sources that a browser makes for the Glass Core Web Profile,
while rejecting malformed expressions without widening access.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-403.md`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- [CSP Level 3](https://www.w3.org/TR/CSP/)

The prior matcher handled `'none'`, `*`, `'self'`, scheme tokens, and
credential-free exact origins. It did not parse the CSP host-source grammar,
so `*.example.test`, source paths, explicit ports, and schemeless host sources
were either ignored or treated as unrelated origins. This affected enforced
and report-only resource checks because both owners share this function.

## Contract

- Preserve the existing semantics for absent source lists, empty source lists,
  singleton `'none'`, and the report-only/non-authorizing boundary. A
  `'none'` token alongside other expressions is ignored, as required by CSP.
- Match scheme sources case-insensitively, including the CSP secure-upgrade
  relation (`http` to `https`, `ws` to `wss`/`http`/`https`, and `wss` to
  `https`).
- Parse host sources with an optional scheme, wildcard host prefix, optional
  numeric or wildcard port, and optional absolute path. Schemeless hosts
  inherit the protected document scheme for matching.
- Match wildcard hosts only below their declared suffix; an exact host source
  remains exact. Compare DNS host names case-insensitively and preserve
  literal address behavior without turning malformed hosts into wildcards.
- Match omitted ports only on the URL's default port, explicit ports against
  that port (including an explicit default), and `*` ports against any
  reachable URL port.
- Match paths using CSP's slash-segment and percent-decoded comparison rules;
  a trailing slash is a segment prefix while another final path is exact.
  Ignore URL query and fragment components.
- Keep `*` bounded to HTTP(S) resources or resources using the protected
  document's scheme, and keep `'self'` restricted to same-origin or the
  specified safe scheme upgrade.
- Reject credentials, malformed schemes/hosts/ports/paths, non-ASCII source
  syntax, and unsupported URL authorities. Invalid expressions never match.
- Keep all downstream owners on the same matcher: page/worker Fetch, CSS,
  images, scripts, frames, workers, EventSource, WebSocket policy URLs, and
  report-only violation checks.

## Non-goals

This slice does not implement nonce/hash grammar changes, `strict-dynamic`
trust propagation, dynamic policy-container mutation, Service Worker policy
propagation, redirect-count-aware path matching, or the full browser security
process boundary. Those remain explicit issue #40 gates.

## Implementation path

- `resource_loader.rs`: add bounded CSP source parsing and scheme/host/port/
  path matching helpers; route `csp_sources_allow` through them while keeping
  WebSocket's actual and normalized policy URL checks intact.
- `resource_loader.rs` unit tests: cover valid scheme and host expressions,
  wildcard boundaries, ports, paths, percent decoding, upgrades, and malformed
  fail-closed cases.
- `tests/native_engine.rs`: retain the existing native HTTP(S) enforcement
  coverage and add one network-owner witness for wildcard/path/port behavior
  if the shared unit contract is not sufficient.
- Update the architecture, plan, analysis, and this task with exact delivered
  behavior and verification evidence.

## Tradeoffs

- A local parser keeps the decision synchronous, dependency-light, and shared
  by every native owner, but it must remain deliberately bounded until the
  conformance corpus covers more of CSP and URL parsing.
- The matcher fails closed for syntax it cannot prove valid. This protects
  origins at the cost of blocking a malformed or future source expression
  rather than guessing.
- Path checks are applied at the current owner boundary because this function
  does not yet receive redirect count. Redirect-aware CSP response matching is
  tracked separately instead of silently ignoring a declared path.

## Delivered

- `csp_sources_allow` now uses a bounded parser for CSP scheme-source and
  host-source expressions rather than treating only exact parsed origins as
  valid. It accepts optional schemes, exact hosts, `*`/`*.` host patterns,
  omitted/numeric/`*` ports, and absolute paths.
- Scheme matching is case-insensitive and includes the CSP safe-upgrade
  relation for HTTP(S) and WebSocket schemes. Schemeless host sources inherit
  the protected document scheme, and `'self'` retains same-origin plus safe
  HTTP(S)/WebSocket upgrade behavior.
- Host matching is case-insensitive with a label boundary for subdomain
  wildcards. Default ports, explicit ports, and wildcard ports are evaluated
  independently of the URL's query or fragment. Path matching follows CSP's
  slash-segment rule, including percent-decoded pieces and trailing-slash
  prefixes.
- Invalid credentials, schemes, hosts, ports, paths, non-ASCII syntax, and
  unsupported authorities fail closed. The CSP singleton `'none'` rule is
  honored without allowing a mixed list to suppress valid expressions.
- The existing page/worker resource, frame, EventSource, WebSocket, enforced,
  and report-only owners all call the same matcher; no CDP fallback or second
  policy implementation was introduced.

## Verification

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --lib csp_ --locked -- --nocapture`
  (8 passed)
- `cargo test --quiet -p glass-browser --test native_engine csp --locked --
  --nocapture` (10 passed)
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked
  --no-fail-fast` (1,119 passed, 1 ignored)
- `cargo fmt --all -- --check`
- `git diff --check`
- `python3 scripts/check-release-documentation.py --require-previous-version
  --report /tmp/glass-release-documentation-404.json`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`

The documentation validators reported 1,054 Markdown documents, 93 routed or
audited current guides, 19 substantive contracts, 15 implementation help
keys, 346 full-product MCP tools, 17 examples, and 22 public modules; current
claim failures were zero.

Remote CI, push, release, tag, and registry publication remain outside this
local checkpoint.
