# Glass native engine browser slice 264: ReadableStream tee

Status: completed locally.

## Objective

Provide the standard two-branch `ReadableStream.prototype.tee()` surface for
native streams so common page and Fetch code can branch a stream without
duplicating transport ownership or allowing unbounded buffering.

## Contract

- `stream.tee()` returns two native readable branches and locks/disturbs the
  original stream through one shared upstream reader.
- Each upstream value is delivered to both live branches, preserving the
  default-reader value type for page-created streams and byte chunks for
  Fetch-backed streams.
- A branch queue is bounded by the existing native stream limit; upstream
  demand pauses while a live branch reaches that limit.
- Upstream close and error propagate to every live branch and settle their
  reader lifecycle state.
- Canceling one branch leaves the upstream alive for the other branch;
  upstream cancellation occurs only after both branches cancel.

## Implementation

The native stream owner now creates a tee coordinator with one upstream reader,
two branch controllers, one in-flight read, bounded branch-state inspection,
and explicit upstream release/cancellation. Branches are ordinary native
underlying-source streams, so their existing lock, reader, `closed`, queue,
and cancellation behavior is reused. Fetch transport streams remain owned by
their existing Fetch stream group while the tee coordinator owns the branch
fan-out.

## Tradeoffs and follow-up

This is a bounded default-reader tee intended for native page and Fetch
workloads. It does not yet implement every Streams standard detail, including
BYOB branch algorithms, custom queuing strategies, transfer/piping APIs, or
the complete Web IDL surface. Queue pressure is explicit and finite so a
stalled branch cannot turn a live source into unbounded memory growth.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_tees_underlying_readable_stream_sources --locked -- --nocapture --exact` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_cancels_tee_upstream_after_both_branches_cancel --locked -- --nocapture --exact` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine tee --locked -- --test-threads=1 --nocapture` (2 passed at initial checkpoint)
- `git diff --check`

The evidence is local-only. Remote CI, release, registry publication, and
final native/CDP parity claims remain pending the wider issue #40 gates.
