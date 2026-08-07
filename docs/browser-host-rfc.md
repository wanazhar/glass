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

The machine-readable dependency and omission matrix is
[`backend-capability-matrix.json`](backend-capability-matrix.json).
