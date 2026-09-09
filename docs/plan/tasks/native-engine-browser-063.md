id: native-engine-browser-063
scope: glass-browser/native-engine/select-multiple-interaction
status: done
depends-on: [native-engine-browser-062]
---

# BE-03ah/BE-04as: bounded multi-select interaction

## Objective

Make native select controls preserve and manipulate multiple selected options
through the same Rust DOM owner used by FormData and script evaluation.

## Contract

- Multiple-select option clicks toggle only the targeted option and emit the
  existing bounded focus/click/change path; single-select clicks continue to
  clear sibling selections.
- Script `option.selected` writes preserve multiple selections, while
  `select.value = value` deterministically selects the first matching option
  and clears other options.
- The script host exposes bounded `multiple`, `options`, and `selectedOptions`
  arrays, with option values taken from the `value` attribute or option text.
- Unsupported semantic targets consistently return `TargetNotActionable` before
  mutation, and the behavior is shared by local and process-backed owners.

## Deliberate boundary and tradeoffs

This is deterministic semantic selection, not full browser listbox behavior:
modifier-key range selection, keyboard navigation, text selection, IME,
`optgroup` disabled inheritance, option collection Web IDL identity, and
native picker rendering remain open. A multi-select click toggles by design so
automation can build a set without depending on platform-specific modifiers.

## Context

- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- GitHub issue #40

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`

## Verification

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_actions_update_state_and_reject_unsafe_targets_before_mutation -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_script_applies_bounded_dom_commands_once -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_form_data_constructor_collects_text_controls -- --nocapture` — 1 passed

Remote CI, source push, release, tag, registry publication, browser parity,
and issue-closure claims remain pending the wider browser-complete gates.
