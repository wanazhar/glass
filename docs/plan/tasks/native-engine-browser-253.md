# Glass native engine browser slice 253: EventSource and SSE transport

Status: completed locally.

## Objective

Give page JavaScript a bounded, persistent Server-Sent Events transport owned
by the native content process. A page must be able to construct an
`EventSource`, receive standards-shaped stream events, reconnect with the
latest event ID, and observe the result without a CDP or alternate-browser
fallback.

## Contract

- `EventSource` construction resolves relative URLs against the committed
  document and accepts only credential-free HTTP(S) targets. The object
  exposes bounded `CONNECTING`/`OPEN`/`CLOSED` state, `url`,
  `withCredentials`, `addEventListener`, `removeEventListener`, the
  `onopen`/`onmessage`/`onerror` handlers, and `close()`.
- The shared native resource loader owns URL, redirect, connect-src/default-src,
  mixed-content, referrer, cookie, and CORS policy. Same-origin cookies are
  sent by default; cross-origin cookies require `withCredentials` and an
  authorized credentialed response. Response `Set-Cookie` changes are handed
  back from the asynchronous stream owner to the content-process loader.
- The content worker owns one bounded stream task per source. It requires a
  successful `200` `text/event-stream` response, parses split LF/CRLF/CR
  records, comments, `event`, multiline `data`, `id`, and bounded numeric
  `retry` fields, and rejects invalid UTF-8 or oversized records.
- Open, named/message, error, and close events re-enter the same serialized
  JavaScript/event/mutation owner as timers, fetch continuations, and
  WebSocket events. Event callbacks can mutate the document, close the source,
  and issue follow-up native work before the snapshot is published.
- Stream disconnects and connection failures emit an error and reconnect with
  the current bounded delay and `Last-Event-ID`; reconnect and channel limits
  prevent a hostile or permanently failing endpoint from creating unbounded
  work. Explicit `close()` terminates the stream without reconnecting.

## Implementation

`NativeScriptCommand` carries typed EventSource open/close operations. The
persistent QuickJS bootstrap owns the source map, state transitions, listener
dispatch, event-handler properties, and bounded event object projection.

`NativeResourceLoader::open_event_source_async` reuses the network policy
owners and returns an unbuffered `reqwest` body stream. The content process
parses the stream in a Tokio task, queues bounded events, transfers response
cookie changes, and dispatches each event through the existing transactional
script mutation loop. The loop only pumps background socket/source events when
the evaluation is explicitly waiting for them; passive evaluations therefore
cannot consume a handler's event before the handler is installed.

## Tradeoffs and follow-up

This slice intentionally keeps bounded channels, message/field limits,
reconnect limits, and a single serialized event owner. It does not claim full
EventSource Web IDL/EventTarget descriptor identity, service-worker
interception, background-page scheduling, complete task-source fairness,
streaming Fetch `Response` parity, or native/CDP parity. Redirect-response
cookie propagation and the remaining browser-complete resource/lifecycle
surfaces remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_drives_ --locked -- --test-threads=1 --nocapture` (4 passed)
- `git diff --check`

The evidence is local-only. The earlier broad 532-test observation, before
the final scheduler and cookie fixes, recorded 526 passes and six failures:
CSP stylesheet request handling, WebSocket event timing, AbortSignal timing,
FormData file rejection, point-click dispatch, and child-frame routing. The
WebSocket timing failure was repaired by the event-pump guard and the focused
four-test rerun is green; the broad gate was not rerun after the final changes,
and the other five failures remain outside this slice. Remote CI, push,
release, registry publication, and final native-engine parity claims remain
unclaimed for this checkpoint.
