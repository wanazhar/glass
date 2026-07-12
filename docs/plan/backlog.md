# Backlog

## Lifecycle: cleanup after implicit incognito-session drop

`BrowserSession::close()` correctly stops a Glass-owned Chrome process before
removing its disposable incognito profile directory. An implicit
`BrowserSession`/`ChromeProcess` drop can only initiate process termination,
so cleanup can race Chrome-held files on platforms such as Windows. Keep the
explicit close contract for library callers, and add an abnormal-shutdown
cleanup mechanism plus a live-session drop regression test in a follow-up.

## Scorecard: cold lifecycle and exhaustive side-effect oracles

Corpus v1 deliberately defines only a warm single-session comparison. Before a
cold scorecard is emitted, define the same process/profile/cache lifecycle for
every adapter and add it to the final comparative acceptance work in
`compare-018`.

The current declarative `forbidden` outcomes catch known wrong-target side
effects and make every such outcome a hard failure. A future corpus should
capture selected target identity or a complete fixture side-effect ledger so
`wrong_actions` is exhaustive rather than limited to enumerated forbidden
values.

## Targeting: remote-handle cleanup on intermediate CDP errors

Bounded CSS/text discovery releases its array and child remote objects on the
successful path. If `Runtime.getProperties` or `DOM.requestNode` fails midway,
the remaining remote handles are left for Chrome's execution-context cleanup.
Add an actor-owned remote-object guard so every partial-error path releases all
objects without increasing the fast reference path's request count.

## Waits: richer diagnostics and network stress instrumentation

Promote Network event lag/domain failures to the same typed wait-error surface
as timeouts, define whether visible-text substrings may span adjacent text
nodes, and add a synthetic CDP stress driver that records Network lease memory
and event-lag behavior under thousands of concurrent requests.

## Topology: typed errors and parallel smoke-test ports

Promote no-selection, stale target/frame, topology-budget, and routing-loss
failures to a bounded typed error that MCP can safely serialize without URLs
or raw oversized IDs. The current explicit behavior is correct but generic
errors give agents less recovery guidance than target and wait failures.

Opt-in browser smoke tests pass serially. Their release-and-reclaim ephemeral
port helper can collide when the suite runs in parallel; replace it with a
process-wide test port lease before making parallel E2E execution a gate.
