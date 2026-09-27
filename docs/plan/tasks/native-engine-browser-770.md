---
id: native-engine-browser-770
scope: glass-browser/native-hyperlink-modifier-clicks
status: in-progress
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
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-770.md`

## Verification

- Add action-level tests for modifier serialization/defaults, event modifier
  visibility, cancellation, exact-once target creation, current `href`,
  download precedence, and non-link behavior.
- Cover local, HTTP(S) content-process, and same-origin-frame execution through
  the native browser action path.
- Cover CLI and MCP modifier parsing, plus explicit unsupported behavior on
  external runtimes.
- Run `cargo fmt --all -- --check`, `git diff --check`, then
  `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  before focused modifier-click tests. Do not run workspace/all-targets tests,
  remote CI, or clean Cargo artifacts for this bounded slice.
