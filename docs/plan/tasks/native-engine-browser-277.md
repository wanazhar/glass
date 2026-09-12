# Glass native engine browser slice 277: bounded script scheduling

Status: completed locally.

## Objective

Correct the execution order of the already-prefetched page-script batch so
async scripts are not unconditionally moved behind every parser-blocking
script, while preserving document order for deferred scripts and modules.

## Contract

- Parser-blocking and async script sources retain their discovery order
  relative to one another.
- Deferred external scripts and module roots execute after that shared
  parser/async sequence in their original discovery order.
- Inline scripts continue to use the existing DOM timing classification;
  inline `async` does not become an asynchronous fetch without a resource.
- Resource `load`/`error` event ownership and failed-script isolation remain
  unchanged.
- The schedule remains deterministic for the current sequential bounded
  resource loader.
- No script source is silently duplicated or discarded by reordering.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- Issue [#40](https://github.com/wanazhar/glass/issues/40)

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/tasks/native-engine-browser-277.md`

## Implementation

`order_page_scripts()` now appends only `Defer` sources to a separate tail.
`ParserBlocking` and `Async` sources stay in the order produced by DOM
discovery and the bounded loader; module roots remain deferred through their
existing classification. The content-process integration witness now
observes `blocking-1-async-blocking-2-defer` for a mixed script document.

## Tradeoffs and follow-up

The native content loader still prefetches external scripts sequentially
before executing the batch, so this is not a full HTML parser/task scheduler.
Real browsers can execute async scripts by network completion order, and
parser-blocking scripts execute while the parser is active. Those races,
streaming parser insertion, async task-source fairness, script fetch priority,
and complete lifecycle/Web IDL semantics remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_honors_bounded_async_and_defer_script_order --locked -- --exact --test-threads=1` (1 passed)

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider issue #40 gates.
