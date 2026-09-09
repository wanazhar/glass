---
id: native-engine-browser-026
scope: glass-browser/native-engine/inline-page-scripts
status: done
depends-on: [native-engine-browser-025]
---

# BE-02h: bounded inline page-script loading

## Objective

Execute the small, deterministic subset of page scripts that is present in a
document at load time, while preserving the existing local-owner and
content-process JavaScript realm split.

## Contract

- A document contributes at most 32 inline `script` elements whose source is
  within the existing native script-byte limit. Empty, JavaScript, and
  `application/javascript` type values are accepted; external `src` scripts,
  module scripts, and other script types are not fetched or executed.
- Local prepared navigations execute inline sources in a fresh owner-side
  realm before commit. A failed source leaves the old navigation committed and
  does not partially publish the new document.
- HTTP(S) documents execute inline sources in the sandboxed content process
  during load. The child retains the resulting globals and listener records
  for later `evaluate_async` and action dispatch; the parent does not rerun
  the sources or attempt to transfer executable realm state over IPC.
- Each source evaluates against the bounded host projection and applies its
  typed commands to a document clone. Link activation is rejected during
  parser-load execution; navigation remains an explicit post-commit owner
  operation.
- The two-crate boundary is unchanged. The helper binary remains an internal
  content-process target, and native-engine remains feature-gated/default-off.

## Deliberate boundary and tradeoffs

- This is not general page loading: external scripts, modules, parser timing,
  dynamic script insertion, timers, Fetch/XHR, CSP script enforcement, and
  full Web IDL identity remain open. Sources are evaluated after the bounded
  document parse, so parser-blocking and incremental DOM timing are not
  represented.
- Inline command effects are intentionally bounded and use the existing
  mutation validation path. This preserves deterministic ownership and
  resource limits at the cost of browser-level script compatibility.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine inline_page_scripts` — 2/2 passed.
- `git diff --check`

The next browser-completeness gates are external script/module policy and
navigation lifecycle/default-action ordering, followed by timers, Fetch/XHR,
and the remaining resource classes.
