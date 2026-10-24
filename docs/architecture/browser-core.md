# Browser core crate

Status: Current workspace architecture.

## Purpose and boundary

`glass-browser-core` contains the native browser engine and the small bounded
contracts shared by browser adapters. `glass-browser` is the user-facing
browser package. It owns browser sessions, backend selection, CLI and MCP
integration, and compatibility re-exports. The development product depends on
`glass-browser`; it does not depend directly on the engine crate.

```text
glass-dev -> glass-browser -> glass-browser-core
```

Dependencies point in one direction. The engine does not depend on the session
adapter or product packages. Shared resource limits live in the core contracts
module. `glass-browser` re-exports older paths so existing Rust imports still
compile.

This crate is separate from the `glass-core` program. `glass-core` runs resident
agent sessions. `glass-browser-core` implements browser behavior.

## Features and integration surface

The default `glass-browser-core` feature set compiles only the shared data
contracts and limit constants. It does not compile the native engine's runtime
dependencies. The `native-engine` feature adds those dependencies and the
engine implementation. `glass-browser` forwards its `native-engine` feature
to this crate.

Engine modules are private implementation details. A small, doc-hidden
integration surface connects the browser adapter to the engine. It does not
make every helper or state field a supported public API. Shared data types
keep their existing serialized field names, size limits, and rejection
behavior.

## Module ownership

The `native_engine` modules follow browser responsibilities: DOM, CSS,
JavaScript, resource loading, content processes, layout, painting, raster
images, navigation lifecycle, and service workers. Large areas use child
modules for separate jobs. For example, CSS cascade and property parsing are
separate, as are DOM tokenization and tree construction. Embedded JavaScript
assets are grouped by browser feature.

For process-backed page and Worker `fetch()`, the parent process owns cookies
and sends a bounded, complete response body over IPC. The child exposes that
body as a `ReadableStream`. This brokered path does not stream bytes from the
socket or cancel the socket request when the child stops reading. The direct
resource loader has separate demand and cancellation behavior; its tests do
not imply those features exist in the process-brokered path.

Native page lifecycle events first check the resident runtime's listener maps
and the native document's inline `on…` attributes. When no handler can observe
an event, the engine records the event without rebuilding the JS document
bridge. The first event still initializes that bridge, and events with a
listener continue through normal evaluation and DOM reconciliation. This keeps
unhandled `load`/`DOMContentLoaded`/`pageshow` events from repeatedly rebuilding
the same bridge on large pages. An opt-in `GLASS_RUNTIME_DIAGNOSTIC_TRACE`
records fixed stage names, elapsed time, and counts in a private profile-local
file; it never records page URLs, bodies, cookies, or script text. It includes
native CLI connect/command/close phases and the parent-brokered document fetch.
On bounded read-only public Amazon navigations, the CLI completed natively at
revision 2 in about 30 seconds. The top-level document fetch took 80–165
milliseconds. A detailed incognito-worker trace attributed about 7.6 seconds
to first document initialization: loading six static system-font candidates
took 1.1 seconds and parsing/loading the bounded 64 discovered faces took
about 7.2 seconds; discovering 375 font paths took 11 milliseconds. HTML5
construction took 5 milliseconds, DOM materialization 1.4 milliseconds, CSS
parsing 86 milliseconds, image loading 311 milliseconds, and document wire
encoding 281 milliseconds.

One resource `load` handler caused the full JS snapshot and layout path. The
snapshot took 8.5 seconds: text projection took 398 milliseconds, element
projection 649 milliseconds, and layout computation 7.1 seconds. Layout tree
building took 3.2 seconds, scroll-container calculation 1.0 second, extent
projection 1.0 second, and geometry projection 331 milliseconds. These are
nested stage durations from one page and one machine; they must not be added to
the overall command time. The font initialization and event-triggered layout
costs are localized but not optimized yet. This does not establish general
browser parity or a latency guarantee.

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
