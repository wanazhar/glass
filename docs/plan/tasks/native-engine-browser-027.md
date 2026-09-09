---
id: native-engine-browser-027
scope: glass-browser/native-engine/classic-external-scripts
status: done
depends-on: [native-engine-browser-026]
---

# BE-02i: bounded classic external page scripts

## Objective

Load and execute ordinary classic external scripts for HTTP(S) documents
through the existing content-process resource-policy owner, preserving their
position relative to accepted inline scripts.

## Contract

- The parsed document exposes at most 32 accepted classic script sources in
  document order. Empty/JavaScript `type` values are classic sources;
  `module`, unknown types, and empty `src` values are not requested.
- The content process resolves each external URL against the document, applies
  the document's `script-src`/`default-src` CSP, HTTPS mixed-content policy,
  bounded redirects, referrer/cookie rules, JavaScript MIME validation, and
  the native script-byte limit before accepting its source.
- Accepted external sources and inline sources execute in document order in
  the same child-owned persistent realm. The parent receives only the final
  bounded document snapshot; globals and listener records remain in the child
  for later `evaluate_async` and action requests.
- External script commands use the existing cloned-document validation path.
  No executable source or callback crosses the content IPC boundary, and no
  path silently falls back to CDP.

## Deliberate boundary and tradeoffs

- This slice is HTTP(S)-content-process only. Synchronous local fixture/data
  navigation remains inline-script-only until it has an explicit local
  subresource table and ownership policy.
- Scripts execute after the bounded document parse in deterministic document
  order. `async`/`defer`/parser-blocking timing, module graphs and dynamic
  `import()`, non-UTF-8 script decoding, inline CSP nonce/hash enforcement,
  timers, Fetch/XHR, and full Web IDL identity remain open.
- Cross-origin classic scripts use the existing CSP/mixed-content policy and
  do not claim a CORS response-read API; broader script isolation and browser
  site-process parity remain production gates.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine external_scripts_in_document_order` — 1/1 passed.
- `git diff --check`

The next script gates are module/dynamic-import policy and parser-timing
semantics, alongside navigation lifecycle/default-action ordering and target
contexts.
