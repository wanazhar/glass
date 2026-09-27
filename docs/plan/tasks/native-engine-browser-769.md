---
id: native-engine-browser-769
scope: glass-browser/keyboard-radio-group-navigation
status: ready
depends-on: [native-engine-browser-768]
---

# Glass native-engine browser slice 769: radio-group arrow navigation

## Objective

Implement keyboard navigation for focused native radio inputs through the
shared `NativeDocument` state owner, including local, HTTP(S) content-process,
and same-origin-frame routes.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) is the source of
  truth for native browser completion.
- The [Glass Core Web Profile](../native-engine-browser-profile.md) declares
  keyboard and form-control behavior across native browser workflows.
- [Slice 768](native-engine-browser-768.md) establishes focused Space
  activation and the shared checkedness/radio-group owner.
- The [HTML Standard radio-group definition and input events](https://html.spec.whatwg.org/multipage/input.html#the-input-element)
  define grouping and selection events, but do not prescribe arrow-key
  direction mapping.
- The [WAI-ARIA Authoring Practices radio-group pattern](https://www.w3.org/WAI/ARIA/apg/patterns/radio/)
  recommends Right/Down for the next radio, Left/Up for the previous radio,
  wrapping at the ends, and moving focus with selection. This slice adopts
  that common profile convention for native radio inputs; APG guidance is not
  itself a normative HTML requirement.

## Contract

- An unmodified `ArrowRight` or `ArrowDown` keydown on a focused, connected,
  enabled native `input[type=radio]` selects the next enabled member of its
  current radio group in tree order. `ArrowLeft` and `ArrowUp` select the
  previous enabled member. Navigation wraps at either end.
- A radio group uses the HTML grouping rules: same tree, same form owner (or
  no form owner), and matching nonempty `name`. Disabled or disconnected
  radios are not navigation destinations. Unnamed radios have no navigation
  group.
- Focus and checkedness move together. Selection events are emitted only when
  checkedness changes: bubbling `input`, then bubbling `change`, on the newly
  checked radio. Arrow navigation does not synthesize pointer events or a
  `click` event.
- `keydown` is dispatched to the current focused radio before the default.
  `preventDefault()` suppresses navigation. After listeners run, revalidate
  focus, attachment, enabled state, type, and current group membership before
  moving focus or selection.
- Unmodified arrow navigation is applied on keydown. A complete non-navigating
  `Shortcut` advances one revision; separately delivered key actions preserve
  their existing per-action revision behavior.
- Local, HTTP(S) content-process, and same-origin frame Documents use the same
  radio group and selection owner. Existing Space activation remains covered.

## Boundaries

This adopts the approved APG radio-group convention for native controls. It
does not implement toolbar-specific radio behavior, ARIA `role=radio` widgets,
Tab entry/exit policy, Home/End shortcuts, access keys, platform-specific
modifier chords, or complete RTL/layout-aware widget navigation. It does not
claim full keyboard or form conformance, WPT completion, remote CI, or
cross-platform certification.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-769.md`

## Verification

- Add DOM-visible tests for forward/backward navigation, wrapping, focus and
  checkedness, unchanged-selection behavior, disabled/unnamed/form-owner
  isolation, keydown cancellation, and listener-time group/focus invalidation.
- Cover local and HTTP(S) content-process execution, plus a same-origin frame.
  Assert event target/order and that no click/pointer event is synthesized.
- Compare the event trace against an already-installed Chromium when available;
  do not install a browser or add a testing dependency for this slice.
- Run `cargo fmt --all -- --check`, `git diff --check`, then
  `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  before the focused `cargo test -p glass-browser --test native_engine
  radio_group_arrow_navigation --locked --quiet` group.
- Do not run workspace, all-targets, or remote CI as part of this bounded slice.

The authoring guide is a user-agent convention rather than the source of the
HTML grouping algorithm. Keep the distinction explicit in design notes and
tests.
