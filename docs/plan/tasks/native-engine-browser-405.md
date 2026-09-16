# Native CSP strict-dynamic script loading (405)

status: done
scope: native-engine/csp-strict-dynamic-trust
issue: 40

## Objective

Make the native engine apply CSP Level 3 `strict-dynamic` rules to page script
elements and their resource-loader handoff. Parser-inserted external scripts
must satisfy a nonce (or an existing supported source rule when
`strict-dynamic` is absent); non-parser-inserted external scripts must be
allowed by `strict-dynamic` without requiring a host allowlist. External script
nonces and report-only decisions must use the same policy semantics.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-404.md`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- [CSP Level 3](https://www.w3.org/TR/CSP/)

The source-expression matcher delivered in 404 still treats every script URL
as an ordinary host/scheme request. That is incorrect for `strict-dynamic`:
the request's parser metadata changes the decision, and a nonce-bearing
parser-inserted script can bootstrap dependency loading. The DOM source model
also currently drops `nonce` from external script elements, preventing the
content process from making that decision.

## Contract

- Parse and recognize the case-insensitive `'strict-dynamic'` keyword in the
  effective script source list (`script-src-elem`, `script-src`, or
  `default-src`). It must not affect style, image, frame, connect, worker, or
  other non-script resource kinds.
- For each enforced policy, a matching external script nonce authorizes the
  script request before URL source matching. A nonce mismatch must not widen
  access.
- With `strict-dynamic`, block parser-inserted script URLs unless their nonce
  matches. Non-parser-inserted script URLs are allowed by the CSP script
  directive even when their host, scheme, or path is absent from the source
  list. Existing mixed-content, URL validation, response, and size limits
  remain in force.
- Without `strict-dynamic`, preserve the 404 source-expression URL decision;
  the external nonce is an additional script-element authorization and does
  not weaken any other resource kind.
- Preserve parser metadata in `NativePageScriptSource`: document-parsed
  scripts are parser-inserted; scripts discovered after a DOM mutation are
  non-parser-inserted. Module external sources use the same boundary.
- Report-only script URL violations must use the same nonce and parser metadata
  rules as enforcement, so a resource allowed by `strict-dynamic` is not
  reported as a host-source violation and a blocked parser script is reported.
- For inline script checks, `'strict-dynamic'` suppresses the broad
  `'unsafe-inline'` allowance for script elements and attributes, while
  matching nonce/hash expressions continue to authorize inline script
  elements. Style inline behavior remains unchanged by the keyword.
- Keep the implementation bounded and fail closed for malformed nonce/source
  expressions. Do not introduce a second CSP matcher or a CDP fallback.

## Non-goals

This slice does not add Subresource Integrity hashing, redirect-count-aware
path matching, policy-container propagation into Service Workers, or the full
browser conformance corpus. Those remain separate issue #40 gates.

## Implementation path

- `dom.rs`: retain external-script nonces and mark parser versus dynamic source
  origins in the page-script source model.
- `resource_loader.rs`: add strict-dynamic and external nonce decisions to
  enforced and report-only script URL checks, and correct inline script
  `unsafe-inline` precedence.
- `content_process.rs`: pass source metadata through external and module
  script loads without changing the shared network/security boundaries.
- Add focused unit tests for policy combinations, then native integration
  coverage for parser and dynamically attached cross-origin scripts.
- Update the architecture, active plan, analysis, and this task with delivered
  behavior and exact verification evidence.

## Tradeoffs

- The native model uses the DOM's parser-versus-mutation boundary as the
  request parser metadata. This is explicit and testable, but later work must
  audit document-write, module-graph, and worker metadata as the conformance
  surface expands.
- Supporting external nonces improves compatibility with strict CSP while
  keeping the URL fetch path shared. Integrity metadata is intentionally not
  guessed or treated as a nonce substitute.
- Strict-dynamic can intentionally authorize arbitrary dynamic script URLs;
  that is the CSP contract and is why the feature is only enabled by an
  explicit policy keyword. URL, mixed-content, response-type, byte, and
  resource-limit gates still protect the native loader.

## Delivered

- `NativePageScriptSource` now preserves external-script nonces and an
  explicit `parser_inserted` bit. Parser-discovered sources set the bit;
  mutation-discovered sources clear it before resource-loader handoff. The
  same metadata applies to classic and module external scripts.
- Enforced script URL checks honor a matching external nonce before source
  matching. When the effective script directive contains
  `'strict-dynamic'`, parser-inserted URLs without a matching nonce are
  blocked, while non-parser-inserted URLs are admitted by the directive
  without requiring a host/scheme/path expression. Mixed-content, URL,
  response-type, redirect, cookie, and byte limits remain unchanged.
- Report-only script URL checks use the same parser metadata and nonce
  decision, so an allowed dynamic URL is not falsely reported as a host-source
  violation and a blocked parser URL is observable. Inline script
  `unsafe-inline` now respects nonce/hash precedence and is suppressed by an
  effective strict-dynamic script list; styles remain unaffected.
- Initial page-script lifecycle evaluations now collect dynamic external and
  module sources, execute queued dynamic inline/module work, and hand external
  sources to the normal content-process loader. This closes the prior drop at
  initial `DOMContentLoaded`/load-time mutation boundaries.

## Verification

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --lib csp_ --locked -- --nocapture`
  (10 passed)
- `cargo test --quiet -p glass-browser --test native_engine strict_dynamic
  --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine
  native_content_process_runs_nested_dynamic_external_scripts --locked --
  --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine
  native_content_process_resolves_dynamic_script_fetch --locked -- --nocapture`
  (1 passed)
- `RUST_MIN_STACK=33554432 cargo test --quiet -p glass-browser --lib --locked
  --no-fail-fast` (1,121 passed, 1 ignored)
- `cargo fmt --all -- --check`
- `git diff --check`

Documentation validators passed with 1,055 Markdown documents, 93 routed or
audited current guides, 19 substantive contracts, 15 implementation help
keys, 346 full-product MCP tools, 17 examples, and 22 public modules; current
claim failures were zero.

The strict-dynamic integration witness confirms that the nonce-bearing parser
bootstrap loads, an unnonceable parser script is not requested, and a
cross-origin non-parser-inserted script executes. The broader native library
and documentation gates remain part of the checkpoint validation. Remote CI,
push, release, tag, and registry publication remain outside this local
checkpoint.
