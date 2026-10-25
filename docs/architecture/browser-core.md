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

Each page script realm has a 256 MiB QuickJS memory cap. Host-view installation
has a separate 15-second execution budget; page script execution retains its
5-second budget. The native snapshot interns equal computed styles and passes
the serialized host state to QuickJS as a temporary runtime string, then
parses and deletes it before page scripts execute. This avoids compiling the
large host snapshot as part of the JavaScript source. A deterministic
2,586-element fixture verifies DOM and
computed-style access within those bounds. It is a local capacity regression,
not evidence of broad public-site compatibility; larger or more dynamic pages
can still exceed the bounded load deadline.

On bounded read-only public Amazon navigations, the CLI completed natively at
revision 2 in about 30 seconds. The top-level document fetch took 96
milliseconds. A process-tagged incognito trace attributed 7.59 seconds to the
content worker's first document initialization: loading six static system-font
candidates took 1.19 seconds and parsing/loading the bounded 64 discovered
faces took 6.39 seconds; discovering 375 font paths took 8 milliseconds. The
host process also initialized the same bounded system-font catalog separately
in 7.84 seconds, so font setup is duplicated across the current process
boundary. In the content worker, HTML5 construction took 5.7 milliseconds,
DOM materialization 2.6 milliseconds, CSS parsing 88 milliseconds, image
loading 408 milliseconds, and document wire encoding 290 milliseconds.

One resource `load` handler caused the full JS snapshot and layout path. The
content worker snapshot took 8.0 seconds: text projection took 409
milliseconds, element projection 494 milliseconds, and layout computation
6.83 seconds. Layout tree building took 2.94 seconds, scroll-container
calculation 1.03 seconds, extent projection 992 milliseconds, and geometry
projection 266 milliseconds. These are nested stage durations from one page
and one machine; they must not be added to the overall command time. The
duplicated font initialization and event-triggered layout costs are localized
but not optimized yet. This does not establish general browser parity or a
latency guarantee.

After host-state JSON moved out of generated JavaScript source and into a
temporary QuickJS runtime string, a polite incognito Wikipedia batch completed
native navigation, observation, a 400-pixel scroll, and a second observation
in about 53 seconds. Eleven host-view installs completed; the slowest took
4.01 seconds, and the final recorded retained QuickJS heap was 128.3 MiB under
the unchanged 256 MiB cap. The batch saw 2,764 content-wire nodes and 1,195
element snapshots. This is one read-only observation path, not broad parity.

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
