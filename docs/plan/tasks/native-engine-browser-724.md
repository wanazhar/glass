---
id: native-engine-browser-724
scope: glass-browser/security/rooted-file-inline-style-csp
status: done
depends-on: [native-engine-browser-723]
---

# Glass native-engine browser slice 724: rooted-file inline style CSP

## Objective

Enforce parser-sourced CSP for inline style elements and style attributes in
configured-root file Documents, including committed JavaScript DOM mutations.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-723.md`
- [CSP Level 3: style-src-elem](https://www.w3.org/TR/CSP/#directive-style-src-elem)
- [CSP Level 3: style-src-attr](https://www.w3.org/TR/CSP/#directive-style-src-attr)
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Contract

- Apply `style-src-elem` → `style-src` → `default-src` fallback to inline
  `<style>` blocks, and `style-src-attr` → `style-src` → `default-src` to
  `style` attributes. Multiple enforced policies remain conjunctive.
- Reuse the shared inline CSP matcher for `'unsafe-inline'`, element nonces,
  and hashes. A style-attribute hash is accepted only with
  `'unsafe-hashes'`; nonces never authorize attributes. `'self'` does not
  authorize inline content.
- Enforce the policy on parser-created file-document styles before the initial
  stylesheet is built, and again after committed runtime mutations that add,
  remove, or change style blocks or attributes. Rebuild CSS from only the
  currently authorized inline sources when style-block sources change, and
  invalidate computed style when style attributes change, so stale
  authorization is not retained.
- Restrict this behavior to configured-root file Documents. Preserve the
  existing HTTP(S) CSP path and file-root admission rules.
- Dynamic CSP meta insertion, report-only file policies, CSS image/font
  subresources, and complete CSP conformance remain outside this slice.

## Path

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-724.md`

## Verification

- Cover directive fallback, policy conjunction, element nonce/hash checks,
  attribute `'unsafe-hashes'` behavior, and rejection of unauthorized inline
  content using loader tests.
- Add process-backed rooted-file tests proving nonce/hash-authorized style
  blocks render, unauthorized blocks/attributes do not, and runtime additions
  or source changes update the applied CSS only when the current source is
  authorized. Preserve the HTTP inline-CSP and rooted-file external stylesheet
  regressions.
- Run one scoped `glass-browser` check before the focused tests, then format,
  whitespace, and maintainer documentation gates. Do not run workspace-wide
  tests or remote CI for this slice.

## Results

Rooted-file inline style elements and attributes now use their directive
fallback chains and the shared nonce/hash matcher. Initial markup, initial page
script mutations, dynamically attached page scripts, and later committed DOM
mutations all re-evaluate file policy. A changed style block rebuilds the
stylesheet from authorized sources; a changed style attribute invalidates
computed style. HTTP(S) policy behavior remains covered by its existing path.

`cargo check -p glass-browser --lib --test native_engine --locked --quiet`
passed. Each focused test passed individually: rooted-file loader policy,
rooted-file process behavior, existing HTTP inline CSP, and rooted-file
external stylesheet CSP. The file process test covers parser styles, initial
page-script mutation, dynamically attached scripts, and later DOM mutations.

`cargo fmt --all -- --check`, `git diff --check`, release-documentation truth
(1,352 Markdown files; zero current-claim failures), documentation depth (93
guides/19 contracts), and shortcut inventory (15 keys/63 markers) passed. The
coverage checker found no broken repository-local links but could not complete
the live CLI inventory because `target/debug/glass` was not built; this slice
changes no CLI, MCP, or command inventory. Remote CI was not run; issue #40
remains open. Dynamic CSP meta insertion, report-only file policies, CSS
image/font subresources, and complete CSP conformance remain separate work.
