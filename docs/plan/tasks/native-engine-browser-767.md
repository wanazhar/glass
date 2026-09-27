---
id: native-engine-browser-767
scope: glass-browser/keyboard-hyperlink-default-actions
status: done
depends-on: [native-engine-browser-766]
---

# Glass native-engine browser slice 767: keyboard hyperlink default actions

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for browser completion.
- The [Glass Core Web Profile](../native-engine-browser-profile.md) requires
  hyperlink navigation and browsing-context behavior.
- [Slice 766](native-engine-browser-766.md) implements focused native-anchor
  Enter activation through the existing click and navigation owners in local,
  process-backed, and same-origin-frame paths. Its contract names existing
  target and download handling but does not verify those special defaults on
  keyboard activation.

## Objective

Verify and, if needed, repair that synthesized Enter activation preserves the
native hyperlink default-action behavior for `_blank` targets and `download`
links across local and content-process execution.

## Contract

- A focused, attached native `<a href>` activated by Enter dispatches its
  cancelable click before applying the link's current default behavior.
- An uncanceled `target="_blank"` link creates exactly one child target with
  the initiating context as opener, while leaving the opener at its original
  URL and active state. Local fixtures and an HTTP-backed content process both
  exercise this path.
- Canceling the synthesized click prevents popup creation and child navigation.
- An uncanceled link with a `download` attribute queues and completes the
  existing download flow with the declared suggested filename and response
  bytes, while leaving the initiating document URL unchanged. The test keeps
  pointer-click download coverage and exercises keyboard activation in the
  same fixture.
- URL policy, popup ownership, download transfer, and file writing remain
  owned by their existing implementations; this slice adds no alternate path.

## Tradeoffs and boundaries

This verifies the two special hyperlink default-action owners reached by
keyboard activation. It does not claim link modifier gestures, image-map area
support, full keyboard/Web Platform Test conformance, cross-platform
certification, remote CI, or issue #40 completion.

## Paths

- `crates/glass-browser/tests/native_engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-767.md`

## Verification

- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
- `cargo test -p glass-browser --test native_engine keyboard_link_default_actions --locked --quiet`
- Existing local script and process-backed pointer popup regressions passed using
  the rebuilt integration-test binary.
- `cargo fmt --all -- --check`
- `git diff --check`
- Maintainer documentation gates: release-documentation audit,
  documentation-depth audit, TUI shortcut inventory, and documentation
  coverage.

The first local end-to-end regression exposed a stack overflow while the
opener's keyboard action synchronously initialized and laid out its popup
target. Target initialization now runs in its own Tokio task and reports task
termination as a typed lifecycle error. The three keyboard-default-action
tests pass on the normal test-thread stack; the existing local script and
process-backed pointer popup regressions also pass.

The target task adds one scheduler spawn per newly created browsing context.
This keeps target ownership and response ordering unchanged while bounding the
initialization stack independently from the initiating action.
