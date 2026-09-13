# Glass native engine browser slice 280: nested dynamic script loading

Status: completed locally.

## Objective

Complete the content-process handoff for external and module scripts created by
scripts that are already executing, so recursive page-script loading stays in
the native owner instead of stopping after the first dynamic insertion batch.

## Contract

- A dynamically executing script may attach an external classic or module
  script and the content owner loads it through the existing URL, origin, and
  resource policy.
- A loaded dynamic module can attach another external/module script, and the
  owner continues loading and executing the newly discovered source.
- Dynamic module source and static module dependencies retain their existing
  bounded module-graph policy.
- Load/error resource events remain associated with the owning script element
  and are delivered through the shared page-script scheduler.
- Dynamic script elements remain single-shot across recursive loader turns;
  the document ledger prevents movement or text mutation from replaying them.
- Recursive loader turns are bounded and fail explicitly when the bound is
  exceeded.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- Issue [#40](https://github.com/wanazhar/glass/issues/40)

## Path

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Implementation

The dynamic page-script result now returns external/module sources discovered
while evaluating an attached script. The content owner drains those sources
through the existing loader, executes the resulting batch, and repeats for
newly discovered sources under the native script/effect limits. Inline
descendants continue through the synchronous shared scheduler, while local
non-network documents reject unsupported dynamic external/module transport
explicitly.

## Tradeoffs and follow-up

This is recursive source loading, not complete browser task scheduling. The
dynamic result can still emit Fetch/WebSocket/EventSource commands that need a
separate event-loop handoff; parser streaming, network completion races,
module graph timing, worker/service-worker ownership, and complete
script/lifecycle/Web IDL semantics remain Issue #40 work.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_runs_nested_dynamic_external_scripts --locked -- --exact --test-threads=1` (1 passed)
- `git diff --check`

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider Issue #40 gates.
