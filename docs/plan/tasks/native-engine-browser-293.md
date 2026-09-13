# Glass native engine browser slice 293: worker EventSource bridge

Status: completed locally.

## Objective

Give dedicated workers a policy-owned persistent Server-Sent Events stream
with the same parser and reconnect transport already used by the page realm.

## Contract

- A dedicated worker can construct a bounded EventSource from an absolute or
  worker-relative URL, configure `withCredentials`, and observe
  `CONNECTING`, `OPEN`, and `CLOSED` state transitions.
- Worker-owned open and close commands carry the worker identifier and cannot
  be applied to a different worker or a page-owned EventSource.
- The HTTP(S) content process resolves the stream through the worker resource
  loader, preserving URL/security policy, cookies, response-cookie changes,
  response limits, named events, multiline data, `lastEventId`, retry state,
  reconnect bounds, and transport errors.
- Open, message, error, and close events run in the isolated worker realm;
  worker callbacks and page messages use the existing bounded page-turn queue.
- Closing the worker EventSource sends a host close command and prevents the
  stream from being left as an unowned background connection.
- A local fixture-owned engine reports that process-backed EventSource
  transport is required instead of silently discarding a worker command.

## Implementation

`NativeScriptCommand` now carries an optional worker owner for EventSource
operations. `NativeWorkerRegistry` retains worker URL context, queues
owner-tagged commands, dispatches serialized SSE events, and collects callback
messages and close commands. The worker bootstrap provides a bounded
EventSource constructor, listener surface, state constants, named-event
dispatch, `lastEventId`, origin, and close behavior. The content process keeps
worker streams separate from page streams, reuses `run_native_event_source`,
applies response-cookie changes to the shared resource loader, and pumps one
worker stream event per page turn.

## Tradeoffs and follow-up

Reusing the existing stream parser and loader keeps cookies, reconnects,
limits, and wire parsing in one transport owner, while the `(worker_id,
source_id)` key prevents cross-realm confusion. Delivery remains page-turn
driven for deterministic local and content-process execution; automatic
background task-source fairness while the page is idle is future work. The
worker surface is bounded and its URL property preserves the supplied text
while the host loader resolves relative URLs. Shared/service/worklet workers,
transferables, complete worker Web IDL parity, and the remaining native/CDP
replacement gates remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_worker_drives_event_source_named_events --locked -- --exact` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine worker --locked` (17 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_drives_event_source_named_multiline_events --locked -- --exact` (1 passed)
- `git diff --check`

The evidence is local-only. Remote CI, release, registry publication, and
final native/CDP parity or production-promotion claims remain pending the
wider issue #40 gates.
