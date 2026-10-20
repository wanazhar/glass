# Browser core crate

Status: Current workspace architecture.

## Purpose and boundary

`glass-browser-core` is the owner of Glass's native browser engine and of the
small bounded contracts shared by browser adapters. `glass-browser` remains
the product-facing browser package: it owns browser sessions, browser backend
selection, CLI integration, MCP integration, and compatibility re-exports.
The development product depends on `glass-browser`; it does not reach into the
engine crate directly.

```text
glass-dev -> glass-browser -> glass-browser-core
```

Dependencies point in one direction. The engine does not depend on the session
adapter or product packages. Resource caps used by both sides live in the core
contract module, with compatibility re-exports retained in `glass-browser` so
existing imports continue to compile.

## Features and integration surface

The default `glass-browser-core` feature set compiles the shared serialized
contracts and limit constants without pulling in the native engine's heavy
runtime dependencies. `native-engine` enables those dependencies and the
engine implementation. The browser package's `native-engine` feature forwards
to the core feature, so existing applications continue to select the native
runtime from the browser package.

Engine modules are private implementation details. A curated, doc-hidden
integration surface connects `glass-browser`'s native backend to the core;
this is an internal crate boundary, not a promise that every engine helper or
state field is a supported public API. Shared DTO serialization and bounded
string deserialization preserve their established field names, bounds, and
rejection behavior.

## Module ownership

The native engine is organized under `native_engine` by browser responsibility:
DOM parsing and mutation, CSS parsing/cascade/stylesheet processing, JavaScript
runtime and host bindings, resource loading, content-process IPC and worker
execution, layout, paint, rasterization, lifecycle, and service-worker state.
Large subsystems use child modules for cohesive responsibilities (for example,
CSS cascade and stylesheet handling, DOM tokenization and tree construction,
JavaScript runtime/profile storage, and content-process IPC/worker lifecycle).
Embedded scripts are kept in domain-named source modules. These boundaries
allow each implementation area to evolve without moving adapter policy into
the engine. Process-backed page and Worker HTTP(S) Fetch remains parent-brokered:
the parent owns cookies and returns a bounded completed response body over IPC.
The child exposes that body through its ReadableStream interface, while
socket-level incremental delivery and transport cancellation are not provided
across this broker path. The direct loader stream transport retains demand and
cancel handling and is covered separately from brokered Fetch behavior.

## Validation

The lightweight contract path is checked with
`cargo check -p glass-browser-core --no-default-features --locked`. The native
implementation path is checked with
`cargo check -p glass-browser-core --all-targets --features native-engine --locked`.
The browser package remains the compatibility and session integration check;
use its no-default-features build to ensure shared contracts do not require the
native engine.

Run `cargo test -p glass-browser-core --all-features --all-targets --locked`
for the core's behavior tests. Build `glass-native-content-worker` from the
browser package first, because process-backed core tests launch that executable.
The shared workspace test and Clippy scripts include the core package, so moving
implementation code across this boundary does not remove it from certification.

See [native engine architecture](native-engine.md), [browser data plane](browser.md),
and [workspace architecture](README.md) for related boundaries.
