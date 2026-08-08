# Semantic resource budgets

Status: Released in `0.3.2`

## Goal

Keep semantic compilation and embedded-agent tooling predictable on remote,
memory-constrained development machines. Correctness and fail-closed behavior
remain authoritative; resource reductions must not weaken evidence or revision
checks.

## Budgets and invariants

1. Task compilation builds graph scope reachability once per selected scope.
   It must not perform a fresh relationship traversal for every authored form
   input.
2. Live binding resolves without allocating a candidate vector per entity.
   Zero and multiple matches remain distinct preflight failures.
3. The agent gateway constructs its descriptor catalog once per gateway, not
   once per call. Public descriptor inspection returns a snapshot without
   changing execution ownership.
4. Tool-call IDs and names are non-empty and bounded before audit or dispatch.
   Argument JSON remains capped at 256 KiB and result JSON at 64 KiB.
5. Private Pi request files are checked through the opened file handle and
   read through a hard byte limit. Metadata checks must not be followed by an
   unbounded read.
6. Project, editor, graph-discovery, and language-server source reads share a
   handle-bound UTF-8 reader. File growth after metadata inspection cannot
   bypass the public 512 KiB or editor 1 MiB limits.
7. Resource optimizations must preserve structured-first observation,
   explicit screenshots, metadata-only audit events, scoped evidence, and
   exact revision guards.

## Verification

- Deterministic tests exercise maximum bounded inputs and dense relationship
  graphs without changing compiled-plan bytes.
- Gateway tests cover oversized identifiers, growing or oversized request
  files, descriptor reuse, authorization, and audit redaction.
- The existing executable Chromium corpus and scoped task scenario remain the
  integration gate.
- A repeatable browser-free benchmark reports compilation throughput and peak
  resident memory for the pinned semantic fixture.

Run the compiler benchmark with:

```bash
GLASS_SEMANTIC_BENCH_ITERATIONS=1000 \
  cargo run --release -p glass-browser --example semantic_resource_benchmark
```

On the local ARM64 release candidate, the 64-input, 66-entity, 129-edge
fixture improved from a 3307.487 microsecond median before the scoped-name
index to 784.361 microseconds after it. The final run reported a 910.041
microsecond p95 and 2268 KiB process peak resident set. Process RSS includes
the Rust runtime and fixture; it is evidence for this pinned workload, not a
global memory guarantee.
