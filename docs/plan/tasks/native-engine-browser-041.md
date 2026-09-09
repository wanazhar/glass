---
id: native-engine-browser-041
scope: glass-browser/native-engine/resource-load-events
status: done
depends-on: [native-engine-browser-040]
---

# BE-02r/BE-03s/BE-04w: bounded resource load completion events

## Objective

Make the existing external-resource and page-script schedule observable at a
typed lifecycle boundary by dispatching successful resource `load` events on
their owning DOM elements before document `DOMContentLoaded`.

## Contract

- Successfully fetched external stylesheet links and external classic/module
  scripts dispatch one non-bubbling, non-cancelable `load` event on their
  corresponding element.
- Resource events are delivered after the accepted stylesheet/script work and
  before the document transitions to `interactive`; callbacks run in the
  existing persistent realm and their bounded mutations commit through the
  existing typed command path.
- Resource completion targets are reconstructed from the committed document,
  sorted by document order, deduplicated, and never accepted as arbitrary
  child-provided node identity.
- The boundary remains deterministic and quota-bounded. A failed or blocked
  resource still follows the existing typed navigation failure/omission rules;
  this task does not claim a complete `error` event model.

## Deliberate boundary and tradeoffs

- This does not implement dynamic resource insertion, image/font/media/fetch
  resource events, resource timing, parser/network concurrency, unload,
  pagehide/pageshow, or wall-clock task scheduling.
- Resource events are emitted at one deterministic post-resource batch point,
  rather than claiming browser-level network completion interleavings.
- The native engine remains default-off inside `glass-browser`; the workspace
  still has exactly two installable crates.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/INDEX.md`

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- synchronized architecture, analysis, plan, and issue #40 records

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --lib --bin glass-native-content-worker --test native_engine`
- focused resource-load integration test
- affected external-script and stylesheet regressions
- external-module and child lifecycle regressions
- `git diff --check`

Remote CI, source push, release, tag, registry publication, browser-parity,
and issue-closure claims remain pending the wider browser-complete gates.
