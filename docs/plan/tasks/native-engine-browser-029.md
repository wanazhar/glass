---
id: native-engine-browser-029
scope: glass-browser/native-engine/module-roots
status: done
depends-on: [native-engine-browser-028]
---

# BE-04k: bounded page-module roots

## Objective

Execute ordinary `type="module"` page-script roots in the same persistent
realm and document order as classic page scripts, while retaining typed native
document ownership.

## Contract

- The bounded page-script discovery path accepts inline and external module
  roots in addition to the existing classic JavaScript types. Unknown script
  types remain ignored, and the existing 32-script/source-size limits still
  apply.
- Local inline module roots execute in a fresh owner-side realm before a
  navigation is published. HTTP(S) inline and external module roots execute in
  the sandboxed content process; globals and listener records remain available
  to later script/action requests.
- Module evaluation uses QuickJS's module evaluator, so module syntax and
  strict-mode semantics are not rewritten into classic scripts. Typed command
  effects still apply to a cloned document and commit only after successful
  evaluation.
- External module source retains its validated final URL as the module name;
  the loader continues to enforce the owning document's script policy,
  mixed-content, redirect, referrer/cookie, MIME, and byte limits.

## Deliberate boundary and tradeoffs

- This is a module-root slice, not a complete module graph. Static imports and
  dynamic `import()` still require the module-resource loader workstream and
  are not silently treated as successful. Local external module subresources
  remain outside the fixture/data navigation contract.
- Module roots are evaluated in the parsed-document batch rather than at true
  parser-blocking/`defer`/`async` timing. Import maps, modulepreload,
  credentials mode, integrity, workers, and full Web IDL identity remain open.
- A module's exported bindings are intentionally not copied into the Rust
  document or global object. The page realm owns them according to ECMAScript;
  only the bounded host command channel crosses back to Rust.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine modules` — 2/2 passed.
- `git diff --check`

The next browser-completeness gates are static module-import graphs,
dynamic-import policy, parser timing/default-action ordering, and the remaining
form and browser-context primitives.
