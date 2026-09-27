---
id: native-engine-browser-775
scope: glass-browser/form-associated-disabled-focus
status: completed
depends-on: [native-engine-browser-774, native-engine-browser-761]
---

# Glass native-engine browser slice 775: form-associated disabled focus state

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for native browser completion.
- The [Core Web Profile](../native-engine-browser-profile.md) requires
  disabled form-associated custom elements to participate in focus behavior
  like disabled form controls.
- [Slice 761](native-engine-browser-761.md) already computes own and inherited
  disabled state for `formDisabledCallback`, including disabled fieldsets and
  the first-`legend` exception.
- [Slice 774](native-engine-browser-774.md) provides shared `tabIndex`
  reflection, while [slice 773](native-engine-browser-773.md) provides generic
  explicit-`tabindex` focus traversal.
- The [HTML Standard's actually-disabled definition](https://html.spec.whatwg.org/multipage/semantics-other.html#disabled-elements)
  includes disabled form-associated custom elements; the focus rules exclude
  actually disabled elements from focusability.

## Objective

Use the custom-element lifecycle owner's computed disabled state in the
Rust-owned native focus path. Preserve the fieldset first-legend exception and
do not treat an ordinary custom element's `disabled` attribute as disabling.

## Contract

- The existing custom-element reconciler sends a typed, bounded native command
  for initial and changed disabled states of autonomous form-associated custom
  elements. It reuses the same computation that drives
  `formDisabledCallback`; it does not introduce a second fieldset algorithm.
- Rust stores the state on its owner node and carries it through content
  process document synchronization. Content-process snapshots reject this
  metadata on non-HTML nodes and HTML elements without a hyphenated local
  name; the custom-element registry remains owned by JavaScript.
- A disabled form-associated custom element with valid explicit `tabindex` is
  excluded from Tab/Shift+Tab traversal and programmatic focus. Dynamic own
  disabled changes and disabled-fieldset changes are reflected at their
  existing reconciliation boundary.
- Descendants of the first `legend` child of a disabled fieldset remain
  enabled. Ordinary custom elements, even with a `disabled` attribute, remain
  eligible when otherwise focusable.
- The local JavaScript projection does not optimistically focus a disabled
  form-associated custom element before its native command is committed.
- Local and HTTP(S) same-origin-frame process-backed paths share the behavior.

## Boundaries and tradeoffs

This carries a single boolean from the existing lifecycle owner rather than
recomputing custom-element registration in Rust. The small state value is
serialized with the node across owner synchronization. It adds no public DOM
attribute or hidden accessibility annotation. Customized built-ins, shadow
focus scopes, `inert`, focus-chain handoff, platform preferences, complete
keyboard behavior, and focus WPT conformance remain open.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-775.md`

## Verification

Passed:

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  (existing unrelated dead-code warnings only)
- `cargo test -p glass-browser --test native_engine --locked --quiet -- disabled_focus_state --test-threads=1`
  (2 focused local and HTTP(S) same-origin-frame regressions)
- `cargo fmt --all -- --check`
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-775.json`
  (1,403 Markdown documents; zero current-claim failures)
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
  (1,403 Markdown files)
- `git diff --check`

Remote CI, Web Platform Tests, cross-platform certification, and issue #40
completion remain open.
