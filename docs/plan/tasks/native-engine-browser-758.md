---
id: native-engine-browser-758
scope: glass-browser/native-custom-elements
status: in-progress
depends-on: [native-engine-browser-757]
---

# Glass native-engine browser slice 758: autonomous custom elements

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) is the authority
  for replacing CDP with the Glass-owned engine.
- The [HTML custom-elements algorithms](https://html.spec.whatwg.org/multipage/custom-elements.html)
  define registry operations, custom-element construction/upgrading, and
  queued lifecycle reactions. The [DOM adoption algorithm](https://dom.spec.whatwg.org/#concept-node-adopt)
  queues `adoptedCallback(oldDocument, newDocument)` for upgraded custom
  elements when their node document changes.
- Native page realms currently expose no working `customElements` registry;
  importing with a non-null custom registry is rejected. `makeElement` already
  builds identity-stable native DOM wrappers, while document mutation methods
  publish ordered native commands. This task connects custom-element state to
  those existing owners and mutation boundaries.
- Slice 757 provides same-tree identity-preserving node transfer. Its normal
  post-adoption route must preserve the custom constructor and deliver the
  adoption reaction exactly once.

## Objective

Implement the global autonomous custom-element registry and its core lifecycle
in native page realms, including parser-created and script-created elements,
ordered reactions for connection, disconnection, observed attribute changes,
and document adoption. The feature must work in local native and isolated
content-process page execution without CDP or a fallback.

## Contract

- Each live native Window/Document generation exposes one stable associated
  `CustomElementRegistry` through `window.customElements`; the registry does
  not leak across a document replacement. Expose `define`, `get`, `getName`,
  `whenDefined`, `upgrade`, and the current-standard `initialize` operation.
- Validate custom-element names, constructor/prototype requirements,
  duplicate names and constructors, definition options, and reentrancy using
  the standard typed exception behavior. Bound definitions, observed
  attributes, pending promises, reaction-queue depth, and callback work.
- Autonomous element construction preserves the existing native node wrapper
  and identity, supports `new CustomElement()` and `document.createElement()`,
  sets the author prototype, and rejects invalid/reentrant construction without
  publishing a partial native mutation.
- Defining a name upgrades eligible existing parser-created elements in tree
  order. `createElement()` and insertion into a connected document attempt the
  appropriate upgrade. `whenDefined()` settles with the registered constructor
  and rejects invalid names with `SyntaxError`.
- Implement the ordered core reactions: constructor/upgrade, observed
  `attributeChangedCallback`, `connectedCallback`, `disconnectedCallback`, and
  `adoptedCallback(oldDocument, newDocument)`. Reactions run at the appropriate
  DOM operation boundary, preserve tree/attribute order, remain bounded under
  callback-driven reentrancy, and do not duplicate host mutations.
- Route set/remove attribute, insert/remove, same-document detach, same-owner
  adoption, and slice-757 cross-owner adoption through the reaction queue.
  A move between connected parents must preserve the standard disconnect then
  connect ordering. Failed definitions or callback exceptions must not leave a
  stale constructor stack or poison later page operations.
- `Document.importNode(node, { customElementRegistry })` accepts the owning
  global registry when allowed by the DOM contract and applies the selected
  registry to the imported element subtree; invalid or unsupported registry
  choices fail explicitly before publication.
- Keep native document state authoritative. Do not emulate custom elements by
  cloning DOM nodes, executing callbacks in a different page realm, or silently
  dropping callback exceptions. Do not add an installable crate or alter the
  CDP migration boundary.

## Boundaries

This slice targets autonomous custom elements and the global registry. It does
not complete customized built-in elements, scoped registries, form-associated
custom elements/`ElementInternals`, custom states, or complete Web Platform
Tests coverage. These remain issue #40 requirements, not exclusions from the
browser-completion objective.

## Tradeoffs

Custom-element callbacks are author code and can recursively mutate the DOM.
Running them directly in the middle of a native document transaction can
observe half-applied state or issue duplicate owner commands. The implementation
must queue reactions at operation boundaries and drain them deterministically;
callback/upgrade work therefore needs explicit limits. The first contract
focuses on the global registry used by ordinary pages while preserving a
separate path for later scoped-registry and form-associated support.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-758.md`

## Verification

- Run `cargo check -p glass-browser --lib --tests --locked --quiet` before
  targeted behavior tests.
- Add focused unit coverage for name/constructor validation, registry
  identity, `whenDefined`, upgrade reentrancy, reaction ordering, exception
  recovery, and reaction bounds.
- Add process-backed native tests for parser-created upgrade, create/construct,
  observed attributes, connected/disconnected callbacks, adoption callbacks
  through slice 757, later script-realm refresh, and clean follow-up operations
  after a failing custom constructor/callback.
- Exercise both in-process and isolated content-worker realms; cover same-origin
  frame page execution where the normal frame owner runs the script.
- Run `cargo fmt --all -- --check`, `git diff --check`, release-truth,
  documentation-depth, TUI-shortcut, and live documentation-coverage gates
  after synchronized documentation edits.
- Record exact local platform/results. Remote CI, Windows/macOS certification,
  full custom-element WPT, and issue #40 closure remain separate gates.

## Current implementation checkpoint (2026-09-26)

Status remains `in-progress` on branch
`task/native-engine-browser-758-template-identities`. The autonomous
custom-element implementation is based on `b4d7af0d` and `6cefcc05`. The
current template-identity follow-up emits a binding command for template
`innerHTML` and pairs JavaScript temporary identities with native nodes while
walking direct and nested template contents. This walk is identity bookkeeping
only; it does not invoke custom-element lifecycle traversal. The regression
covers direct `template.innerHTML`, a nested template in ordinary
`innerHTML`, an additional nested template, cross-script wrapper identity,
later attribute mutations, and inert lifecycle behavior.

The third rehydration correction unconditionally calls
`fragment.__glassRefresh(entry)` for cached and new fragments in
`fragmentNodes`, before `nodesByIndex` is built and child links are restored.
This assigns the native snapshot index to a cached fragment created while its
template host had a temporary index, while retaining the fragment object and
host reference used by `template.content`.

Local verification on Linux aarch64 (`Linux 6.17.0-1018-oracle`):

- `cargo fmt --all`: passed.
- `git diff --check`: passed after the synchronized documentation update.
- `cargo check -p glass-browser --lib --tests --locked --quiet`: passed.
- `cargo test -p glass-browser --test native_engine native_custom_elements_upgrade_create_and_run_lifecycle_reactions --locked --quiet`:
  exact output: `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured;
  791 filtered out; finished in 19.65s`. This was the third focused run, after
  `fragmentNodes` unconditionally called `fragment.__glassRefresh(entry)` for
  cached and new fragments before rebuilding tree children. The cached
  fragment object is retained and receives its native node index, so the
  template host's retained `content` reference resolves its rehydrated
  children. The two earlier focused runs failed at the script boundary. The
  broader task remains in progress. Remote CI and cross-platform
  certification were not run.
