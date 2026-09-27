---
id: native-engine-browser-770
scope: glass-browser/native-hyperlink-modifier-clicks
status: complete
depends-on: [native-engine-browser-769]
---

# Glass native-engine browser slice 770: modifier-aware hyperlink clicks

## Objective

Expose primary-click modifier state through the normal semantic action path and
use it for native hyperlink context selection across local, HTTP(S)
content-process, and same-origin-frame documents.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for browser completion.
- The [Glass Core Web Profile](../native-engine-browser-profile.md) owns the
  explicit Control/Meta/Shift mapping and documents where it differs from
  platform UI behavior.
- The [HTML Standard hyperlink algorithm](https://html.spec.whatwg.org/multipage/links.html#following-hyperlinks)
  defines target-navigable selection from hyperlink attributes; it does not
  prescribe platform modifier-click behavior.
- [W3C UI Events](https://www.w3.org/TR/uievents/) defines the mouse-event
  modifier attributes exposed to page listeners.
- Slice [769](native-engine-browser-769.md) is the preceding merged input
  checkpoint. Existing native click paths already own cancellation, downloads,
  URL policy, popup target creation, and frame routing.

## Contract

- The normal semantic click request carries Alt, Control, Meta, and Shift
  independently. Existing callers that provide no modifier state retain
  ordinary-click behavior.
- The cancelable click event exposes the requested modifier flags before any
  navigation or target default action. `preventDefault()` suppresses the
  default action, including creation of a modifier-selected target.
- An uncanceled primary click on a live hyperlink with Control, Meta, or Shift
  creates exactly one background native browsing context and leaves the source
  context selected. Multiple selected modifiers still create only one target.
  Existing explicit target behavior must not create a duplicate target.
- The link's current post-listener `href`, existing URL/navigation policy, and
  existing `download` attribute behavior remain authoritative. Alt alone does
  not implicitly download a link without the explicit download behavior.
- Non-link clicks retain their existing default behavior while exposing the
  modifier state to page listeners.
- Local, HTTP(S) content-process, and same-origin-frame actions use the same
  modifier and target-selection contract.
- Provide the modifier option through the canonical Rust semantic action,
  Glass CLI click inputs, and the native MCP click tool. Unsupported external
  runtimes must fail explicitly instead of silently dropping modifier state.

## Tradeoffs and boundaries

The HTML hyperlink algorithm does not define Ctrl/Meta/Shift gestures. GCWP
chooses one portable headless behavior: all three open a background browsing
context. Glass does not model OS windows versus tabs or activate the new
context. Alt-click does not trigger an implicit download. Right/middle-button
input, physical down/up sequencing, complete pointer-event conformance, WPT
completion, cross-platform certification, remote CI, and issue #40 completion
are outside this slice.

## Paths

- `crates/glass-browser/src/browser_backend.rs`
- `crates/glass-browser/src/browser/native_engine/interaction.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/src/browser/runtime.rs`
- `crates/glass-browser/src/cli/args.rs`
- `crates/glass-browser/src/cli/runner.rs`
- `crates/glass-browser/src/mcp/server.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/actions.md`
- `docs/cli.md`
- `docs/features.md`
- `docs/mcp-tools.md`
- `docs/rust-sdk.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-770.md`

## Results

- `cargo fmt --all -- --check` and `git diff --check` pass.
- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  and `cargo check -p glass-dev --lib --bins --locked --quiet` pass.
- `cargo test -p glass-browser --lib modifier --locked --quiet --
  --test-threads=1` passes: 8 tests, including action/event modifiers, CLI and
  MCP parsing, and fail-closed behavior on adapters that cannot preserve click
  modifier state.
- `cargo test -p glass-browser --test native_engine modifier --locked --quiet
  -- --test-threads=1` passes: 4 tests covering local activation/cancellation,
  content-process target/download behavior, same-origin frame routing, and the
  keyboard modifier-event contract.
- The download test creates and removes its exact unique destination
  directory. The keyboard trace expects `shiftKey=true` for the observed
  `Shift+Tab` keyup.
- Documentation release audit passes for 1,398 Markdown files with 0 current-
  claim failures; depth audit validates 93 guides and 19 substantive contracts;
  shortcut inventory validates 15 implementation keys and 63 doc markers.

These are local focused results only. Remote CI, cross-platform certification,
WPT conformance, complete pointer input, and issue #40 completion remain open.
