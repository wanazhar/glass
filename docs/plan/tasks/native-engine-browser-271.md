# Glass native engine browser slice 271: static module dependency failure isolation

Status: completed locally.

## Objective

Keep a normal HTML navigation usable when an external module root was fetched
but one of its bounded static dependencies cannot be loaded. The failure must
be observable on the root `<script type="module">` element without evaluating
an incomplete graph or aborting the document commit.

## Contract

- Static module dependencies are prefetched before the module root is
  evaluated.
- If a dependency fetch, policy/MIME check, URL resolution, or graph-bound
  check fails, the incomplete root graph is removed from the evaluation set.
- The external module root receives one bounded non-bubbling `error` event and
  its source does not run.
- The owning HTML document still commits and reaches its normal ready-state,
  DOMContentLoaded, and window-load phases.
- A fully prefetched graph keeps the existing module evaluation and `load`
  event behavior.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- Issue [#40](https://github.com/wanazhar/glass/issues/40)

## Path

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/tasks/native-engine-browser-271.md`

## Implementation

The static dependency walker now treats an unavailable dependency as a failed
module graph rather than propagating the subresource error out of the whole
document load. Callers stage each root and its dependencies in the shared
script list; a failed walk truncates that staged range. External module roots
then replace their provisional `load` event with one `error` event, while
successful graphs retain the existing order and QuickJS module loader.

Inline modules also discard an incomplete staged graph, without changing the
existing external-root event surface. This keeps the current typed resource
boundary and avoids evaluating a partially prefetched module set.

## Tradeoffs and follow-up

The slice intentionally handles root-graph prefetch failure as a resource
failure. It does not yet provide full inline-module element error identity,
browser-grade module fetch/task scheduling, import maps or bare specifiers,
service-worker interception, or complete module/Web IDL semantics. Those remain
issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine module --locked -- --test-threads=1 --nocapture` (4 passed)

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider issue #40 gates.
