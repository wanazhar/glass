---
id: native-engine-browser-777
scope: glass-browser/focus-transition-events
status: completed
depends-on: [native-engine-browser-776]
---

# Glass native-engine browser slice 777: bubbling focus transition events

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [UI Events focus event order](https://www.w3.org/TR/uievents/#events-focusevent-event-order)
  places `focusin` after `focus` and `focusout` after `blur`; the bubbling
  focus events carry the opposite transition endpoint in `relatedTarget`.
- The [Core Web Profile](../native-engine-browser-profile.md) requires
  focus changes to behave consistently across local documents, content
  processes, and same-origin frame projections.

## Objective

Complete the bounded focus transition event sequence for native focus
operations, including bubbling, `relatedTarget`, and process/frame projection.

## Contract

- A newly focused target receives `focus` followed by `focusin`; a previous
  target receives `blur` followed by `focusout` before the new target's events.
  `focus` and `blur` do not bubble; `focusin` and `focusout` bubble.
- For a paired transition, old-target events reference the new target and
  new-target events reference the old target. Unpaired focus entry/exit has a
  null `relatedTarget`.
- The Rust event kind, host event metadata, content-process codec, and
  same-origin frame event projection preserve the event names and related node
  identity. Frame projection resolves the related node in the child document.
- Script `focus()`/`blur()` and native Tab/Shift+Tab and radio-arrow focus
  transitions dispatch the same bounded sequence.
- Internal focus events are `FocusEvent` instances with the bounded
  `UIEvent.view`/`detail` and `FocusEvent.relatedTarget` surface.
- Native owner transitions continue to dispatch each event separately so
  listener commands are applied at their existing reconciliation boundary;
  transition metadata is computed from the complete old/new pair without
  collapsing it into one host turn.
- Local, HTTP(S) same-origin-frame content-process, and parent-projected frame
  regressions cover direct focus, Tab, order, bubbling, interface identity, and
  related targets.

## Boundaries and tradeoffs

This establishes the bounded event path, not full focus Web IDL or focus
algorithm conformance. Reentrant focus requests during listeners, shadow-DOM
retargeting, cross-origin related-target rules, platform focus-chain handoff,
and complete focus WPT coverage remain separate requirements.

## Paths

- `crates/glass-browser/src/browser/native_engine/interaction.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-777.md`

## Verification

Passed:

- `cargo fmt --all -- --check`
- `cargo check -p glass-browser --lib --locked --quiet` (existing unused parser
  item warnings only)
- `cargo test -p glass-browser --test native_engine --locked --quiet -- focusin_focusout --test-threads=1`
  (2 local and HTTP(S) same-origin-frame process/parent-projection regressions;
  43.62 seconds)
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-777.json`
  (1,405 Markdown documents; zero current-claim failures)
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
  (1,405 Markdown files)
- `git diff --check`

Remote CI, Web Platform Tests, cross-platform certification, and issue #40
completion remain open.
