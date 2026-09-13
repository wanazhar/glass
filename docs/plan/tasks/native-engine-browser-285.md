# Glass native engine browser slice 285: worker importScripts dependencies

Status: completed locally.

## Objective

Allow ordinary classic workers to use a bounded static dependency graph. A
worker's `importScripts()` dependencies must load through the native resource
and security owner and execute before the root source in the same isolated
realm.

## Contract

- Static `importScripts()` calls with string-literal URLs are discovered from
  worker source while ignoring strings, comments, and property names.
- Dependencies load relative to the importing worker resource through the
  existing worker URL, redirect, cookie, MIME, mixed-content, byte, and
  worker-src policy boundary.
- Nested dependencies are bounded by the existing module-import limit and
  cycles fail as typed worker network errors.
- Imported source executes before its importing source in the same QuickJS
  realm, preserving globals and the worker message/lifecycle owner.
- Each preloaded string call is consumed by the worker's `importScripts`
  function; an unpreloaded or non-literal later call throws explicitly.
- Dependency load, parse, and evaluation failures preserve the existing worker
  error event boundary and never fall back to CDP.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- Issue [#40](https://github.com/wanazhar/glass/issues/40)

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Implementation

The native worker registry now performs a bounded depth-first resource walk
before creating the worker realm. It collects per-call raw specifier counts,
rejects cycles and oversized combined sources, and evaluates child source
before parent source. The worker bootstrap receives those counts and exposes
`importScripts`; each matching preloaded call consumes one count, while a
dynamic or otherwise unknown call throws a typed JavaScript error.

This keeps dependency loading in the existing two-crate native ownership
model. No worker resource is fetched from JavaScript, and no source is
evaluated in the page realm.

## Tradeoffs and follow-up

The graph is a bounded lexical preload, not a complete JavaScript parser or
call-position scheduler. It is designed for ordinary top-level classic worker
libraries; dependencies called conditionally or from later functions are
preloaded ahead of time and do not re-execute at the original call position.
Dynamic expression resolution, exact importScripts evaluation timing, worker
Fetch/XHR, module/shared/service workers, transferables, and complete Worker
Web IDL semantics remain issue #40 expansion work.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_worker_preloads_import_scripts_dependencies --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine worker --locked -- --nocapture` (11 passed)
- `git diff --check`

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider Issue #40 gates.
