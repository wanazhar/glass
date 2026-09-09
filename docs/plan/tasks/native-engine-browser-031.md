---
id: native-engine-browser-031
scope: glass-browser/native-engine/literal-dynamic-imports
status: done
depends-on: [native-engine-browser-030]
---

# BE-04m: bounded literal dynamic imports

## Objective

Support literal `import("...")` calls for module code whose dependency is
already admitted by the bounded content-process module graph.

## Contract

- Literal dynamic-import URLs are discovered during the same bounded module
  prefetch pass as static imports. Relative and absolute HTTP(S) URLs resolve
  against the importing module's final URL.
- The QuickJS module loader resolves the prefetched source and preserves normal
  module namespace/export behavior. A bounded pending-job drain delivers the
  resulting promise callbacks before page-script publication completes.
- The existing document script policy, mixed-content, redirect,
  referrer/cookie, MIME, UTF-8, graph-entry, graph-byte, memory, stack, and
  timeout limits remain in force.

## Deliberate boundary and tradeoffs

- Only literal string specifiers are prefetched. Computed expressions, bare
  package specifiers, import maps, modulepreload, workers, and non-HTTP(S)
  modules remain typed/open; they are not given an implicit unrestricted
  network path.
- The job drain is bounded and runs after module-root evaluation, so this is
  not a full browser task/microtask scheduler or parser-timing implementation.
  Unsettled work beyond the bound remains outside the current contract.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine literal_dynamic_imports` — 1/1 passed.
- `git diff --check`

The next browser-completeness gate is parser-blocking/defer/async timing and
bounded task/microtask ordering, followed by full form lifecycle and browsing
contexts.
