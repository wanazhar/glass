# Glass native engine browser slice 281: dynamic network effects

Status: completed locally.

## Objective

Route network effects emitted by dynamically loaded page scripts through the
same native content event loop as ordinary page evaluations, so dynamic Fetch,
WebSocket, and EventSource behavior is not rejected or silently discarded.

## Contract

- A dynamically loaded script may issue `fetch()` and its Promise
  continuation is resolved by the existing bounded content-process fetch loop.
- Dynamic WebSocket open/send/close commands install into the existing
  persistent connection owner and retain its event-delivery policy.
- Dynamic EventSource open/close commands install into the existing persistent
  SSE owner and retain its event-delivery policy.
- DOM, navigation, dialog, scroll, and error effects produced before or after
  a dynamic network continuation remain part of the same published mutation.
- Dynamic network requests remain subject to the existing URL, origin, CORS,
  credential, timeout, body, response, and transport bounds.
- The resolver does not consume queued background events merely because a
  connection exists; event pumping remains controlled by the requesting turn.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- Issue [#40](https://github.com/wanazhar/glass/issues/40)

## Path

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Implementation

Dynamic page-script execution now returns pending Fetch/WebSocket/EventSource
commands after committing its DOM effects. The content owner activates
persistent transports and queues dynamic Fetch requests into the existing
resolver. Stateful content-script turns use that resolver even when their
initial command list has no network operation, allowing network commands
created by a dynamically evaluated script to be handled before publication.
Background event pumping remains distinct from connection existence so
listeners installed by a later evaluation still receive queued events.

## Tradeoffs and follow-up

Using the unified resolver adds a bounded handoff pass to stateful
content-script evaluations, trading a small amount of per-turn overhead for a
single network-effect ownership path. Local data/document URLs still reject
external dynamic transport explicitly. Parser streaming, true network
completion ordering, complete stream/body Web IDL semantics, worker/
service-worker ownership, and full native/CDP parity remain Issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- dynamic script Fetch witness — 1 passed
- nested dynamic module/external witness — 1 passed
- existing page Fetch, timer Fetch, WebSocket, and EventSource witnesses — 1 passed each

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider Issue #40 gates.
