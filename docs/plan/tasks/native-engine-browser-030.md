---
id: native-engine-browser-030
scope: glass-browser/native-engine/static-module-graphs
status: done
depends-on: [native-engine-browser-029]
---

# BE-04l: bounded static module graphs

## Objective

Prefetch and execute bounded static module dependencies without transferring
executable state across the content-process boundary.

## Contract

- The content process performs a bounded lexical discovery of static
  `import ... from`, side-effect `import`, and `export ... from` specifiers.
  Relative and absolute HTTP(S) module URLs are resolved against the importing
  module's final URL.
- Each dependency is fetched through the existing owner-document script policy,
  mixed-content, redirect, referrer/cookie, MIME, UTF-8, and byte limits. A
  bounded set of validated final-URL/source pairs is installed into QuickJS's
  in-memory module loader before the root executes.
- Cycles and duplicate URLs are bounded by the graph-entry limit. Dependencies
  are available to module linking but are not independently executed as page
  roots; QuickJS owns import/export binding and evaluation order.
- The parent receives only the final typed document snapshot. No source,
  module object, binding, or callback crosses the IPC boundary.

## Deliberate boundary and tradeoffs

- Bare specifiers require import maps and are rejected until import-map policy
  exists. Dynamic `import()` is not prefetched or exposed as a fake success;
  workers, modulepreload, integrity, non-HTTP(S) module URLs, parser timing,
  and local external module subresources remain open.
- The discovery pass is deliberately limited to URL prefetching; QuickJS is
  still authoritative for JavaScript grammar. This keeps the loader bounded,
  but malformed or unusual syntax may fail at module evaluation rather than
  during prefetch discovery.
- The graph is fetched before root evaluation, so parser-blocking and network
  concurrency timing do not yet match a full browser event loop.

## Paths

- `crates/glass-browser/Cargo.toml`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine static_module_graphs` — 1/1 passed.
- `git diff --check`

The next browser-completeness gates are dynamic-import policy, parser timing
and task ordering, POST/submission lifecycle, and target browsing contexts.
