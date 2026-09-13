# Glass native engine browser slice 279: dynamically attached scripts

Status: completed locally.

## Objective

Execute page scripts that are attached after the initial document lifecycle,
including create-now/attach-later flows, while preserving the browser
single-shot rule for each script element.

## Contract

- A newly attached classic inline script executes synchronously in the page
  realm when the host DOM operation attaches it to the document.
- A script element created in one evaluation and attached in a later
  evaluation is discovered and executed when it becomes connected.
- Moving or changing an already-started script does not execute it again.
- Dynamically attached module and external script sources enter the bounded
  native page-script path for the initial completed host mutation; process
  backed HTTP(S) documents resolve their authorized external source and
  resource events through the existing loader.
- The started-script ledger survives content-process document-wire transfer
  and rejects invalid or duplicate script identities at the owner boundary.
- Dynamic script errors are isolated to the script element and reported
  through the existing bounded error-event path.

## Context

- `docs/INDEX.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- Issue [#40](https://github.com/wanazhar/glass/issues/40)

## Path

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Implementation

The native document now carries a bounded started-script ledger across local
and content-process turns. Host mutations identify newly connected script
subtrees, including elements created before attachment. The JavaScript
bootstrap synchronously evaluates connected classic inline scripts and emits
a host marker so the Rust scheduler does not evaluate the same element twice.
The shared dynamic scheduler executes module/classic sources, dispatches
bounded resource/error events, and preserves generated navigation, dialog,
scroll, and page-event effects. The process path loads direct dynamic
external/module sources through the existing resource policy before dispatch.

## Tradeoffs and follow-up

This slice does not claim HTML parser `innerHTML` script execution, full
network task timing, or complete script/Web IDL semantics. External or module
scripts created by a dynamically executing script are discovered by the
bounded document ledger but still need an event-loop loader handoff; dynamic
Fetch/WebSocket/EventSource effects currently fail explicitly rather than
being silently dropped. Parser streaming, async completion races, worker/
service-worker script ownership, and complete browser parity remain open
Issue #40 gates.

## Verification

- `cargo fmt --all -- --check`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine dynamic_inline_script --locked -- --test-threads=1` (2 passed)

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider Issue #40 gates.
