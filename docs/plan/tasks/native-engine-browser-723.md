---
id: native-engine-browser-723
scope: glass-browser/security/rooted-file-stylesheet-csp
status: completed
depends-on: [native-engine-browser-722]
---

# Glass native-engine browser slice 723: rooted-file stylesheet CSP

## Objective

Enforce parser-sourced CSP on external stylesheets and recursive CSS imports
loaded by configured-root file Documents.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-722.md`
- [CSP Level 3: style-src-elem and fallback](https://www.w3.org/TR/CSP3/#directive-style-src-elem)
- [HTML Standard: link resource fetch and nonce metadata](https://html.spec.whatwg.org/multipage/links.html)
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- Reuse slice 722's parser-sourced policies, installed before file resources
  load. Enforce `style-src-elem`, `style-src`, then `default-src` fallback for
  initial and dynamically attached/updated external stylesheet links, and for
  every rooted-file CSS `@import` dependency. Multiple policies remain
  conjunctive.
- For file resources, `'self'` matches resources admitted by the same
  most-specific configured file root as the Document. A matching link nonce
  may satisfy its own stylesheet request; it does not authorize CSS imports.
  Explicit `file:` source expressions remain bounded by configured-root
  admission. SRI and the existing CSS byte/graph limits remain enforced.
- Reject a disallowed stylesheet before reading its bytes. Preserve the
  existing element load/error event behavior and never route file resources
  through network transport.
- Do not change HTTP(S) CSP behavior.
- This slice does not cover inline `style` elements or attributes, blob/data
  stylesheets, CSS image/font subresources, report-only file policies,
  dynamically inserted CSP meta elements, or complete CSP conformance.

## Path

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/backlog.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-723.md`

## Verification

- Add loader coverage proving rooted-file `style-src-elem`/`style-src`/
  `default-src` fallback, same-root `'self'`, matching link nonce behavior,
  conjunctive policies, and denial before reading invalid-UTF-8 CSS bytes.
- Add process-backed file-document tests for an allowed initial stylesheet,
  denied initial/dynamic stylesheet requests and their error events, an
  allowed same-root import, and a blocked import in a different configured
  root. Assert blocked rules do not affect computed style.
- Run one scoped `glass-browser` check before focused tests. Then run only the
  new rooted-file CSP tests and directly affected stylesheet regressions,
  formatting, whitespace, and maintainer documentation gates. Do not run
  workspace-wide tests or remote CI for this slice.

## Results

Implemented the style-source checks in the rooted-file stylesheet loader, so
the initial and dynamic link paths and recursive `@import` dependencies share
the same pre-read CSP gate. Link nonce metadata is forwarded for link fetches;
imports do not inherit it. CSP rules are conjunctive and use configured-root
`'self'`; existing root admission, SRI, graph/byte limits, and load/error events
remain in place.

`cargo check -p glass-browser --lib --test native_engine --locked --quiet`
passed. The two loader unit tests and two new process-backed CSP tests passed;
the process-backed test observes the initial blocked link's error effect and
both dynamically blocked links' JavaScript error listeners. The existing
unrooted-import test and rooted file stylesheet/script/image regression passed
individually. Formatting and `git diff --check` passed.
`cargo fmt --all -- --check`, `git diff --check`, release-documentation truth
(1,351 Markdown files; zero current-claim failures), documentation depth (93
guides/19 contracts), and shortcut inventory (15 keys/63 markers) passed.
The coverage checker verified repository-local Markdown links but could not
complete live CLI/MCP inventory checks because the scoped clean-target
validation did not build `target/debug/glass` or `target/debug/glass-browser`.
Those binaries are unchanged by this slice. Remote CI was not run; the branch
remains local-only. Inline style elements/attributes, blob/data
stylesheets, CSS image/font subresources, report-only file policy, and full CSP
conformance remain open issue #40 work.
