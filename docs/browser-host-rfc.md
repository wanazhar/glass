# Glass Browser Host RFC

## Status and scope

This RFC defines the host boundary for Pillar III backend survivability. A
Browser Host owns transport startup, command serialization, lifecycle, and
bounded endpoint discovery. The host exposes only the transport-neutral
`BrowserBackend` contract from `src/browser_backend.rs`; CDP and WebDriver BiDi
wire types remain below their adapters.

The deterministic `semantic-proof` backend is a conformance backend. It is
useful for protocol tests, but it is not browser parity and MUST NOT be
reported as a real browser.

## Registration and selection

A host registers typed `BackendStartup` candidates and calls
`BackendFactory::start` with a validated `BackendSelectionRequest`. Selection
is deterministic:

1. an explicit backend preference is strict and never silently falls back;
2. automatic selection orders certification and capability coverage;
3. backend id is the stable final tie-breaker.

The returned `StartedBackend` carries both the selected machine-readable
profile and the owned adapter. Every dispatch is capability-gated by
`BrowserBackendDispatcher`; an omitted or disabled capability returns the
stable `CapabilityUnavailable` error.

## BiDi startup and command envelope

`BidiBrowserBackend::connect_with_config` accepts a `ws://` or `wss://` endpoint.
For an `http://` or `https://` endpoint it performs bounded discovery and
requires a `webSocketUrl` (case-compatible `websocketUrl` is accepted). The
WebSocket command envelope is `{id, method, params}` and responses are matched
by id. Events, ping/pong frames, payload size, message count, and command time
are bounded; malformed or mismatched responses fail closed as typed connection
errors.

The certified BiDi slice is intentionally small:

- `session.new` and `session.end` lifecycle;
- `browsingContext.getTree` contexts;
- `browsingContext.navigate` navigation;
- `script.evaluate` for bounded script and evidence extraction;
- bounded DOM click/type action translation;
- revision-based effects and verification through evidence.

Capture, storage, prompts, downloads, key presses, and scrolling remain
unavailable until a capability declaration and deterministic conformance test
exist. A disabled script capability also disables evidence and action.

## Survivability and authority

One serialized command stream is retained per backend. The adapter retains only
current URL, active context, and a monotonic revision; it does not persist
page payloads by default. Transport reconnection is not inferred: after a
closed stream, lifecycle and command calls fail closed rather than replaying a
mutation. The current Web IR, revision, policy, and capability evidence remain
executable authority; backend profiles are declarations, not permission to
bypass those checks.

## Current browser runtime mapping

The full `BrowserSession` remains the Chromium/CDP production path. The public
`BrowserRuntimeSession` adds a deliberately smaller portable path:

| Runtime | Transport | Status | Startup |
|---|---|---|---|
| Chromium | Chrome DevTools Protocol | Production full session | Glass launches or explicitly attaches |
| Firefox | WebDriver BiDi WebSocket | Experimental portable semantics | User starts Firefox with `--remote-debugging-port` and supplies `--browser-endpoint` |
| Safari | W3C WebDriver HTTP through `safaridriver` | Experimental portable semantics | User starts `safaridriver` and supplies its base URL |
| Native | Glass-owned in-process Rust engine | Experimental local semantics; feature-gated | Explicit `native-engine` build; `NativeEngineConfig` or the local CLI path; no endpoint |

The external portable command set is navigation, one active context, compact
script-derived evidence, script evaluation, CSS click/type actions, and
revision effects. The native command set is local navigation, one active
context, bounded URL/title/visible-text evidence, semantic click/type actions,
and bounded native point hit testing plus revision effects; it does not execute
script. Screenshots, storage,
prompts, downloads, keyboard, scrolling, multi-window control, profiles, MCP,
TUI, and the full locator/Web IR pipeline remain capability-denied on these
adapters.

Firefox is configured as a browser-specific BiDi profile so selection can
require `browserFamily=firefox`; Safari is intentionally represented by the
classic WebDriver adapter because SafariDriver is not currently a certified
direct BiDi endpoint in this codebase.

Protocol references:

- [W3C WebDriver BiDi](https://www.w3.org/TR/webdriver-bidi/)
- [MDN: create a WebDriver BiDi connection](https://developer.mozilla.org/en-US/docs/Web/WebDriver/How_to/Create_BiDi_connection)
- [WebKit: WebDriver is coming to Safari](https://webkit.org/blog/9395/webdriver-is-coming-to-safari-in-ios-13/)

## Native browser feasibility

The native-engine program now has real Phase 2 semantic interaction and
initial Phase 3 presentation/layout/display-list/software-surface slices
behind the default-off `native-engine` feature. It owns one deterministic
in-process context, local `about:blank`, `data:text/html`, and registered
`fixture://` resources, a small DOM/text projection, history, revisions,
bounded CSS presentation, integer normal-flow rectangles, point hit testing,
semantic click/type actions for local controls, a bounded effects signal,
bounded inherited text color, paint clips, side-specific solid/dashed/dotted-border paint,
bounded physical circular border radii, bounded inline-box line placement,
bounded outer/content box geometry with uniform padding/margin and explicit
box sizing, bounded vertical viewport scrolling, and bounded PNG capture through the explicit backend operation,
plus Rust-only display-list/software-surface artifacts. It does not yet provide
general CSS/nested/horizontal/stacking layout, four-side padding/margin, negative/percentage/auto box
model values, positioned/flex/grid layout,
screen-shot-containing evidence, JPEG/PDF capture, or physical-pixel capture,
font/image fidelity, JavaScript, network/security policy, cookies/storage,
downloads, or platform windowing.

`BrowserRuntimeSession` remains the transport adapter for externally managed
Firefox and Safari. With the `native-engine` feature, the native backend is
also exposed through the explicit `BrowserRuntimeSession::connect_native` Rust
constructor and the local one-shot `--browser-runtime native` path. The CLI
default configuration accepts only `about:blank` and bounded `data:text/html`;
Rust callers may register `fixture://` documents. Native never accepts an
external endpoint, enters automatic selection, or falls back to another
backend. The native backend remains a multi-year architecture project with
its own standards conformance, security review, process isolation, and
platform certification. The proof backend must never be presented as browser
parity.

The machine-readable dependency and omission matrix is
[`backend-capability-matrix.json`](backend-capability-matrix.json).
