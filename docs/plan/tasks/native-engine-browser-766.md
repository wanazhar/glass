---
id: native-engine-browser-766
scope: glass-browser/native-anchor-keyboard-activation
status: done
depends-on: [native-engine-browser-765]
---

# Glass native-engine browser slice 766: native anchor keyboard activation

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for browser completion.
- The [Glass Core Web Profile](../native-engine-browser-profile.md) includes
  keyboard input and ordinary hyperlink navigation.
- Slices [764](native-engine-browser-764.md) and
  [765](native-engine-browser-765.md) implement focused button activation and
  explicitly leave native anchors out.
- The [HTML Standard hyperlink activation algorithm](https://html.spec.whatwg.org/multipage/links.html#following-hyperlinks)
  defines native `a[href]` activation. The [WAI-ARIA Link Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/link/)
  documents Enter as the link execution key.

## Objective

Make Enter activate a focused native `<a href>` through the same synthesized
click and existing Glass link-navigation paths in local and process-backed
Documents.

## Contract

- A focused, attached native `<a>` with an `href` activates on Enter keydown
  only. It dispatches `keydown`, synthesized `click`, then `keyup`.
- Canceling keydown suppresses click dispatch. Canceling the click suppresses
  navigation. Click handlers may update the anchor `href`; the post-handler
  value is used by the hyperlink default action.
- The activated anchor must still be the focused attached `a[href]` after
  keydown dispatch. Document replacement, removal, or loss of `href` suppresses
  activation.
- A complete non-navigating `Shortcut` remains one revision; navigation retains
  the existing event-then-navigation commit path.
- Local and process-backed top-level Documents use existing URL resolution,
  navigation policy, target, download, and history owners. Same-origin frame
  activation must continue through the owning frame route.
- Space does not activate a native hyperlink. Anchors without `href`, `area`
  image-map links, and ARIA-only link roles are outside this slice. Modifier
  gestures that select a new browsing context are also outside this slice.

## Tradeoffs and boundaries

This closes the native-anchor Enter gap only. It does not claim complete
keyboard conformance, image-map focus support, modifier-key context creation,
remote CI, cross-platform certification, or issue #40 completion. Network
regressions use a local HTTP server; local-document regressions use configured
fixtures and existing navigation policy.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-766.md`

## Verification

Passed locally:

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine keyboard_link_activation --locked --quiet`
  (local fixture, process-backed HTTP, and same-origin child-frame tests)
- `cargo fmt --all -- --check`
- `git diff --check`
- Maintainer documentation gates: release-documentation audit,
  documentation-depth audit, TUI shortcut inventory, and documentation
  coverage.

The tests verify keydown/click/keyup ordering, keydown and click cancellation,
post-click `href` mutation, Space non-activation, local and HTTP navigation,
and child-frame routing. Process-backed cancellation also verifies that the
content worker transfers the click's allowed/canceled result to its owner. URL
resolution, navigation policy, target/download handling, and history remain
owned by the existing click/navigation paths. These local checks do not
establish remote CI, cross-platform certification, complete keyboard
conformance, or issue #40 completion.
