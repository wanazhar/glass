# Glass native engine browser slice 257: Fetch body ownership

Status: completed locally.

## Objective

Bring native `Response` body ownership closer to the Fetch contract. A body
must become disturbed when JavaScript consumes it, repeated body methods must
fail, and cloning must fail once the original body is locked or disturbed.
Valid clones created before consumption must continue to own independent
bounded readers.

## Contract

- `Response.bodyUsed` is false for a fresh response and becomes true after a
  body method or a `ReadableStream` reader consumes or cancels its body.
- `text()`, `json()`, `blob()`, `arrayBuffer()`, and `bytes()` claim the
  response body once. A second body-method call returns a rejected `TypeError`
  instead of replaying the retained body.
- `Response.clone()` rejects with a `TypeError` when the response body is
  locked or disturbed. Clones created before either condition retain separate
  response/body state and can be consumed independently.
- Acquiring a reader locks the response body for clone/body-method checks;
  merely acquiring and releasing an unused reader does not set `bodyUsed`.
  Reader reads, reader cancellation, and unlocked stream cancellation disturb
  the underlying body state.
- Filtered opaque/opaque-redirect and null-body responses retain their
  existing filtered projections and do not invent a readable body.
- Shared tee ownership, BYOB readers, piping, trailers, full queuing-strategy
  semantics, and complete Fetch Streams/Response Web IDL identity remain
  separate promotion work.

## Implementation

The persistent page realm now records response body state and links it to the
native `ReadableStream` state. Body methods claim the response before asking
the existing retained-history/transport owner for bytes; stream readers mark
the body disturbed on read/cancel; and clone checks inspect both disturbance
and lock state. The compatibility witness was rewritten to create all
parallel body readers before consuming the original, and a focused HTTP
integration witness covers fresh, consumed, locked, canceled, and cloned
responses.

## Tradeoffs and follow-up

This is single-response disturbance and clone gating, not a full Fetch tee.
The current bounded clone implementation still shares the transport group and
retains bounded history; cancellation waits for sibling clone/body consumers
to release the group. A later slice should replace that approximation with
explicit tee branches and cancellation propagation, then add BYOB/piping,
trailers, stream queuing strategies, and descriptor-level Web IDL coverage.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine fetch --locked -- --test-threads=1 --nocapture` (17 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_tracks_fetch_body_disturbance_and_clone_ownership --locked -- --exact --test-threads=1 --nocapture` (1 passed)
- `git diff --check`

The evidence is local-only. Remote CI, push, release, registry publication,
and final native/CDP parity claims remain pending the wider issue #40 gates.
