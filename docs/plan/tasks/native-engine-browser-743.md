---
id: native-engine-browser-743
scope: glass-browser/descendant-frame-unload-lifecycle
status: done
depends_on: [native-engine-browser-742]
---

# Glass native-engine browser slice 743: descendant-frame unload lifecycle

## Objective

Make cross-document navigation of a native browsing context run the outgoing
lifecycle for that frame and its descendant frames as one navigation attempt.
Carry the HTML sandboxed-modals restriction through the native frame tree so
only an eligible, permitted `beforeunload` prompt can pause or cancel that
attempt. This is a production-path behavior slice, not a Core Web Profile or
browser-completion claim.

## Context

- `docs/INDEX.md`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-741.md`
- `docs/plan/tasks/native-engine-browser-742.md`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- HTML Standard: [preventing navigation] and [unloading documents]
- HTML Standard: [iframe sandbox attribute]

## Contract

- Before a cross-document navigation replaces a frame, dispatch `beforeunload`
  in that frame and each active descendant frame. Finish all these checks before
  dispatching `pagehide`/`unload`, loading the replacement, or changing history.
  Same-document navigations remain in-place and do not run this unload tree.
- A navigation attempt has one shared prompt budget. A canceled
  `beforeunload` event can open the generic browser-controlled prompt only when
  its Document has sticky activation, its active sandbox flags permit modals,
  and no prompt has already been shown for this attempt. Cancellation without
  those conditions does not itself block navigation. A later eligible frame
  may use the prompt if an earlier canceled event was ineligible.
- Dispatch all affected `beforeunload` handlers even after a prompt is shown or
  dismissed. Dismissal prevents every affected frame's `pagehide`/`unload`,
  replacement request, and history commit; callback mutations remain on the
  outgoing documents. Acceptance permits the one navigation to continue.
- On continuation, dispatch `pagehide` and `unload` child-before-parent and
  persist each outgoing frame's local storage before publishing the replacement.
  No frame may receive these events twice for the same navigation.
- A live Document keeps the sandboxed-modals bit it received when it became
  active. Changing or removing its iframe owner's `sandbox` attribute does not
  retroactively change that outgoing Document. When a cross-document frame
  navigation activates a replacement, compute its bit from the current owner
  attribute: a present `sandbox` token set without ASCII-case-insensitive
  `allow-modals` adds the restriction; an absent attribute does not. Preserve
  sandboxed-modals restrictions inherited from ancestor Documents even when a
  nested iframe includes `allow-modals`. This slice does not promote the rest
  of iframe sandbox security.
- Apply the shared lifecycle gate to native BrowserSession navigation,
  explicit frame navigation, page/window-target navigation, reload, and
  cross-document Back/Forward. Keep the current target/frame owner and route
  through the native engine only; do not probe, start, or fall back to CDP.
  If a parked ancestor is the navigation target while a descendant is selected,
  include that selected Document in the same lifecycle attempt and restore the
  prior selection if the attempt is dismissed or does not replace the Document.
- A lifecycle-triggered navigation must not recursively dispatch the same
  outgoing tree a second time. Preserve typed rejection for unsupported
  multiple lifecycle navigations and preserve cancellation-safe state.
- Process-backed prompts retain exact context/frame identity. Missing a
  responsive prompt controller remains an explicit error, never implicit
  acceptance.

## Tradeoffs

The frame registry owns the active sandboxed-modals bit for each live Document;
the bit is refreshed for a replacement from its current iframe owner and
inherited ancestor restrictions, not inferred from a page-controlled
`returnValue`. Frame lifecycle callbacks are serialized in stable tree order
by the native owner, while the HTML Standard permits queued tasks across
distinct event loops. Broader sandbox flags, frame-origin policy, dynamic frame
creation/removal, and complete navigation/task scheduling remain separate
issue #40 requirements.

## Path

- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/tests/native_engine.rs` or focused native backend tests
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-743.md`

## Verification

- With nested frames, prove `beforeunload` reaches each active Document once
  before any replacement request; prove `pagehide`/`unload` run deepest-first
  only after the confirmation decision.
- Navigate a parked ancestor while its grandchild is selected; verify the
  grandchild's prompt blocks the ancestor navigation and dismissal restores the
  same selected frame and full outgoing frame tree.
- Dismiss one descendant's exact-ID prompt and prove all frame URLs, history,
  and documents remain active, callback mutations persist, and no frame gets
  `pagehide`/`unload` or a target request.
- Accept a prompt and prove a single prompt is shown for the navigation even
  when multiple activated frames cancel; all `beforeunload` handlers still run
  and all eligible descendants then unload once.
- Prove canceled events without activation continue without a prompt; a
  sandboxed frame without `allow-modals` cannot open one; `allow-modals`
  permits it; a changed owner attribute takes effect only for the replacement
  Document; and a nested child cannot clear an inherited restriction.
- Cover the shared gate for canonical BrowserSession navigation, explicit
  frame navigation, target/window navigation, reload, and cross-document
  Back/Forward. Same-document fragments and history traversal remain in-place.
- Run one affected-package `cargo check` before focused tests, then formatting,
  documentation truth/depth/coverage checks, and `git diff --check`. Distinguish
  Linux-local evidence from remote CI and Windows/macOS certification.

## Evidence

- `cargo check -p glass-browser --test native_engine --locked --quiet` passed
  on Linux.
- The focused parked-ancestor/grandchild prompt test passed: dismissal keeps
  the selected grandchild and outgoing frame tree active; acceptance replaces
  the ancestor and removes descendants. The focused inherited-sandbox test
  passed: a nested `allow-modals` cannot lift an inherited restriction, an
  owner token edit does not affect the active Document, and the replacement
  Document receives the updated eligibility.
- Native frame lifecycle and sandbox-token unit tests passed.
- `cargo fmt --all -- --check`, release-documentation truth, documentation
  depth and coverage, TUI shortcut inventory, and `git diff --check` passed
  locally.
- This is Linux-local slice evidence only. It does not establish remote CI,
  Windows/macOS behavior, full iframe sandbox security, or Core Web Profile
  completion; those remain issue #40 gates.

[preventing navigation]: https://html.spec.whatwg.org/multipage/browsing-the-web.html#preventing-navigation
[unloading documents]: https://html.spec.whatwg.org/multipage/document-lifecycle.html#unloading-documents
[iframe sandbox attribute]: https://html.spec.whatwg.org/multipage/iframe-embed-object.html#attr-iframe-sandbox
