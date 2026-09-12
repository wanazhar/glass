# Glass native engine browser slice 258: Fetch clone queues

Status: completed locally.

## Objective

Give pre-consumption `Response.clone()` branches explicit bounded queue
ownership. A fast reader must not force the content worker to grow an
unbounded queue for a paused sibling, and canceling one branch must not abort
the transport needed by another live branch.

## Contract

- Each live native response-body stream state owns a separate bounded queue of
  received 8 KiB parts. A chunk is broadcast once to each non-canceled,
  non-body-method branch; branches consume their own queue independently.
- A new transport read is requested only while every live stream branch is
  below the queue bound. If one branch is paused at the bound, the shared
  response waits until that branch reads or cancels.
- A response consumed by a body method is removed from stream-branch queue
  accounting; it continues through the retained bounded body history and does
  not accumulate a duplicate stream queue.
- Canceling or returning one reader clears that branch and leaves the shared
  transport alive while another stream branch or body consumer remains live.
  The final live branch cancellation sends the existing typed transport
  cancellation command.
- The existing response-size, chunk-size, command-count, and event-loop
  bounds remain authoritative. Filtered/null-body responses are unchanged.
- The full Fetch Streams tee algorithm, BYOB readers, piping, trailers,
  queuing strategies, and complete Web IDL identity remain open.

## Implementation

The persistent JavaScript stream group now gates each demand against the live
branch queue bound and excludes branches already claimed by a response body
method. The content worker retains unserved parts from the current transport
chunk in a bounded queue and emits one part only after the next typed read
command. Existing group cancellation keeps the underlying response alive for
clone siblings and closes it after the final branch cancels.

The integration witness creates two readers from one response, verifies that
both receive the same body part, cancels the first reader without closing the
transport, then cancels the second and verifies the server observes closure.

## Tradeoffs and follow-up

The queue bound is deliberately finite, so a paused clone can apply stronger
backpressure than a browser's unbounded tee queue. This protects the native
content worker from memory growth but means callers that clone a response must
service or cancel every branch. The retained history still supports bounded
body convenience methods and later pre-consumption clones. A later slice
should model explicit tee branches and cancellation promises, then add BYOB,
piping, trailers, and complete Fetch Streams/Web IDL semantics.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_tees_fetch_streams_until_all_readers_cancel --locked -- --exact --test-threads=1 --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine fetch --locked -- --test-threads=1 --nocapture` (18 passed)
- `git diff --check`

The evidence is local-only. Remote CI, push, release, registry publication,
and final native/CDP parity claims remain pending the wider issue #40 gates.
