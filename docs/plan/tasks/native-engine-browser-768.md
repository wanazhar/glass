---
id: native-engine-browser-768
scope: glass-browser/keyboard-checkable-activation
status: done
depends-on: [native-engine-browser-767]
---

# Glass native-engine browser slice 768: keyboard checkable activation

## Objective

Make Space activate a focused native checkbox or radio input through the
existing native click/default-action owner, for local, process-backed, and
same-origin-frame Documents.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) is the source of
  truth for native browser completion.
- The [Glass Core Web Profile](../native-engine-browser-profile.md) requires
  keyboard and form-control behavior across native browser workflows.
- Slices [764](native-engine-browser-764.md) and
  [765](native-engine-browser-765.md) establish Space-on-keyup activation for
  buttons; [766](native-engine-browser-766.md) and
  [767](native-engine-browser-767.md) establish keyboard link activation and
  its special defaults.
- The [HTML Standard's input activation behavior](https://html.spec.whatwg.org/multipage/input.html#the-input-element)
  defines checkbox/radio activation, legacy pre-activation, canceled
  activation restoration, and the `input`/`change` effects.

## Contract

- An unmodified Space sequence on a focused, attached, enabled native
  `input[type=checkbox]` or `input[type=radio]` dispatches `keydown`, `keyup`,
  then exactly one cancelable synthesized `click`. Enter does not activate
  these controls in this slice.
- The keydown must be uncanceled, and the same supported control must remain
  focused, attached, and enabled through keyup. Focus changes, detachment,
  disablement, type changes, or document replacement suppress the pending
  activation. A canceled keydown also suppresses it.
- Checkable legacy pre-activation is visible to click listeners. If click is
  canceled, checkedness and checkbox indeterminateness are restored and no
  `input` or `change` event is emitted.
- An accepted checkbox activation toggles checkedness and clears
  indeterminateness. An accepted radio activation checks the target without
  toggling an already-checked target off, and clears checkedness only for
  members of its radio group (same tree, form owner, and nonempty name).
- After an accepted click activation on a connected control, dispatch bubbling
  `input` then `change` in that order. Space on an already-checked radio keeps
  it checked (it does not toggle off) but still completes that accepted radio
  activation. Activation must not submit, validate, or navigate a form.
- `Shortcut` retains one revision for a complete non-navigating sequence;
  separately delivered `KeyDown` and `KeyUp` retain their existing per-action
  revisions and pending-focus identity checks.
- Local fixture, HTTP(S) content-process, and same-origin frame paths use the
  same state/default-action owner. Existing pointer/semantic click behavior
  remains covered if the shared checkable activation owner changes.

## Boundaries

This is a bounded checkable-keyboard slice, not complete keyboard or form
conformance. Platform-specific modifier shortcuts, accessibility activation,
radio-group direction-arrow navigation, form-control Web IDL reflection, WPT
suite completion, cross-platform certification, remote CI, and issue #40
promotion gates remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-768.md`

## Verification

- `cargo fmt --all -- --check` and `git diff --check` passed on this worktree.
- `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo check -p glass-browser --lib --test native_engine --locked --quiet` passed.
- Focused checkable tests cover four cases. Three passed in the filtered run; the remaining local test exposed a fixture reset omission after its prior case changed a radio's name. After restoring that name in the setup, `CARGO_TARGET_DIR=/home/ubuntu/work/glass/target cargo test -p glass-browser --test native_engine native_local_keyboard_checkable_activation_orders_events_and_restores_canceled_state --locked --quiet -- --exact` passed: 1 passed, 0 failed, 817 filtered out. No production change followed the passing package check.
- Maintainer documentation gates: release-documentation audit,
  documentation-depth audit, TUI shortcut inventory, and documentation
  coverage. The release-truth audit validated 1,396 Markdown documents with
  zero current-claim failures; depth validated 93 current guides and 19
  substantive contracts; shortcut inventory validated 15 implementation help
  keys and 63 markers; coverage validated 1,396 Markdown files, 346 MCP tools
  (101 browser-only), 17 examples, and 22 public modules.

The first package check caught one visibility error in the accepted-click
connectedness test; the content process now uses the existing connected
checkable predicate, and the required package check passes. Focused tests cover
local checkbox/radio event order and rollback, local raw-key cancellation and
focus/type/disabled invalidation, HTTP process behavior, and same-origin-frame
activation. The implementation also keeps an accepted activation's
`input`/`change` dispatch when its click listener disables the still-connected
control. This remains a local verification checkpoint; remote CI and complete
keyboard/form conformance are still open.

All Cargo commands follow the repository's Rust execution policy: complete a
coherent implementation batch before checking; use package-scoped `cargo
check` before targeted tests; do not compile after individual edits.
