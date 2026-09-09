---
id: native-engine-browser-042
scope: glass-browser/native-engine/navigation-lifecycle
status: done
depends-on: [native-engine-browser-041]
---

# BE-02s/BE-03t/BE-04x: bounded replacement navigation lifecycle

## Objective

Close the first full-replacement navigation lifecycle boundary for the
single-context native engine by delivering outgoing and incoming page lifecycle
events through the existing local and child-owned realm paths.

## Contract

- A full replacement navigation delivers `pagehide` to the outgoing window,
  then `unload`, before the new resource is requested/committed.
- The newly committed page delivers `pageshow` to its window after its accepted
  script/ready/load schedule and parent publication. Initial page publication
  also delivers `pageshow`.
- Lifecycle events are non-bubbling and non-cancelable, and callback commands
  remain bounded and typed. Outgoing callback mutations are committed before
  the old page is discarded; incoming callback mutations use the existing
  persistent-realm mutation path.
- Local and child owners expose the same ordering. Transition records use the
  bounded effects channel so callers can audit `pagehide`, `unload`, and
  `pageshow` without reading page data.
- Same-document fragment navigation does not dispatch replacement lifecycle
  events.

## Deliberate boundary and tradeoffs

- This does not implement cancelable `beforeunload`, `hashchange`, bfcache
  persistence, history traversal lifecycle parity, popup/opener contexts,
  document visibility, or the full HTML navigation task model.
- The child lifecycle command is serialized and deterministic; it does not
  claim browser-level network/task interleavings.
- Lifecycle callbacks cannot silently initiate a second navigation during the
  transition; unsupported command phases fail through the existing typed path.
- The native engine remains default-off inside `glass-browser`; exactly two
  installable crates remain.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/INDEX.md`

## Paths

- `crates/glass-browser/src/browser/native_engine/interaction.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- local replacement lifecycle test 1/1
- child replacement lifecycle test 1/1
- existing local and child ready-state lifecycle regressions 1/1 each
- `cargo fmt --all -- --check`
- `git diff --check`

Remote CI, source push, release, tag, registry publication, browser-parity,
and issue-closure claims remain pending the wider browser-complete gates.
