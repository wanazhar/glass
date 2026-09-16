# Native parser-time CSP meta-policy composition (398)

```yaml
id: native-engine-browser-398
scope: native-engine/content-security-policy-meta-composition
status: done
depends-on:
  - native-engine-browser-397
```

## Objective

Make parser-time enforced Content Security Policy complete across the current
native document loading path. HTTP response policies and parser-discovered
`<meta http-equiv="Content-Security-Policy">` policies must all apply to the
same document, with every enforced policy constraining the request or inline
execution decision. The frame policy crossing the content-process boundary
must preserve that intersection rather than flattening it into a permissive
source union.

## Contract

- Parse only parser-discovered CSP meta elements in the document head; ignore
  report-only meta values and meta elements outside the head.
- Preserve the existing bounded CSP source matcher and fallback semantics for
  each individual policy.
- Enforce all response-header and CSP-meta policies for styles, scripts,
  images, fonts, media, frames, connections, workers, inline elements, and
  inline attributes wherever the current loader already owns that decision.
- Replace parser-time meta policies on each document load so cached or
  Service-Worker-served documents cannot accumulate stale policies.
- Carry multiple frame source groups through the typed content-process result
  and require every group to admit a child navigation.
- Keep policy source bytes out of logs and preserve existing bounds and
  fail-closed handling for malformed or oversized policy input.
- Do not claim report-only violation events, `SecurityPolicyViolationEvent`,
  strict-dynamic trust propagation, or the complete CSP source grammar.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-398.md`

## Tradeoffs

The implementation keeps the existing Rust-owned source-expression matcher
and transports frame policy as bounded source groups. This adds a small typed
protocol and allocation cost, but it preserves the security invariant that
multiple enforced policies are an intersection. It intentionally leaves
reporting, dynamic policy mutation, strict-dynamic, and unsupported grammar
for separate conformance slices instead of silently widening this subset.

## Delivered

- Added head-only parser discovery for enforced CSP meta elements while
  ignoring report-only meta declarations and elements outside the head.
- Composed every response-header and parser-time meta policy as an
  intersection for the existing subresource and inline policy owners.
- Replaced meta policy state on each content-document load so cached and
  Service-Worker-served navigations do not accumulate stale declarations.
- Carried multiple frame source groups through the typed content-process
  response and required every group to admit a child navigation.
- Added policy-unit, DOM-discovery, HTTP(S) inline-resource, and HTTP(S)
  frame-navigation witnesses.

## Verification

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --lib csp --locked -- --nocapture`
- `cargo test --quiet -p glass-browser --test native_engine csp --locked -- --nocapture`
- `cargo fmt --all -- --check`
- `git diff --check`
- release-documentation, documentation-depth, TUI-shortcut, and
  documentation-coverage validators

Remote CI, push, release, tag, and registry publication remain outside this
local-only checkpoint.
