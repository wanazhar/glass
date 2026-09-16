# Native document inline CSP enforcement (395)

```yaml
id: native-engine-browser-395
scope: native-engine/document-inline-csp
status: done
depends-on:
  - native-engine-browser-394
```

## Objective

Close the inline-content security gap in the HTTP(S) content owner. A
response's Content Security Policy must govern inline script and style
execution just as it already governs external subresources.

## Contract

- Preserve CSP source-expression bytes that carry security material; nonce and
  hash values must not be lowercased or otherwise normalized before matching.
- Apply `script-src-elem` (falling back to `script-src`, then `default-src`)
  to classic and module `<script>` elements. Inline sources are admitted only
  by `unsafe-inline`, a matching nonce, or a matching supported hash.
- Apply `style-src-elem` (falling back to `style-src`, then `default-src`) to
  `<style>` elements and external stylesheet links. Inline style elements use
  `unsafe-inline`, a matching nonce, or a matching supported hash.
- Apply `style-src-attr` (falling back to `style-src`, then `default-src`) to
  `style="..."` declarations. Attributes require `unsafe-inline`, or a
  matching hash when the policy also carries `unsafe-hashes`.
- Enforce the same inline-script policy for dynamically attached script
  elements in the content process. Blocked sources must not be evaluated and
  must project the existing bounded script error event.
- Enforce the same style policy after DOM mutation so blocked style elements
  and attributes cannot re-enter computed style, background-image discovery,
  or diagnostics through a later script turn.
- Keep local fixture/data documents' existing no-response-policy behavior and
  preserve all existing URL, subresource, byte, and process boundaries.

## Explicit follow-up

This slice does not claim the complete CSP specification. `script-src-attr`,
`meta` policy, report-only delivery, violation-event reporting, strict-dynamic
trust propagation, import-map/module-policy interactions, and full CSP source
grammar remain separate issue #40 security/conformance work.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/css.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-395.md`

## Tradeoffs

The implementation keeps CSP decisions in the Rust resource loader and uses
the existing document snapshot/command boundary, so no policy-bearing header
or page source is logged or sent through a new transport. The bounded hash
set is limited to SHA-256/SHA-384/SHA-512 and exact source bytes; broader CSP
grammar and reporting need a later versioned contract rather than silently
accepting an expression that this owner cannot verify.

## Delivered

- Rust CSP policy ownership now preserves nonce/hash bytes, applies the
  element-specific directive fallback chain, and supports bounded SHA-256,
  SHA-384, and SHA-512 inline hashes.
- HTTP(S) content loads filter blocked `<style>` elements and `style="..."`
  declarations before stylesheet, background-image, diagnostics, computed
  style, and script-snapshot projection.
- Initial and dynamically inserted classic inline scripts use the same
  policy decision; blocked scripts are not evaluated and dispatch the
  existing element error event.
- The script adapter carries the style admission bit so page-side
  `getComputedStyle()` cannot reapply a blocked raw style attribute.

## Verification

- `cargo check --quiet -p glass-browser --tests --locked`
- Focused loader CSP unit test: 1 passed.
- Focused content-process CSP integration test: 1 passed.
- Persistent inline-script and resource-event lifecycle regressions: 1 passed
  each.
- CSP loader group: 3 passed.
- `cargo fmt --all`, `git diff --check`
- Release documentation, documentation depth, TUI shortcut, and coverage
  validators all passed with zero current-claim failures.

Remote CI, push, release, tag, and registry publication remain outside this
local checkpoint.
