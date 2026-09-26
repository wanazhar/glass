---
id: native-engine-browser-759
scope: glass-browser/native-custom-elements
status: complete
depends-on: [native-engine-browser-758]
---

# Glass native-engine browser slice 759: customized built-in elements

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) remains the source
  of truth for the native engine's browser-completion objective.
- The [Glass Core Web Profile](../native-engine-browser-profile.md) includes
  HTML custom elements as required DOM behavior; the autonomous registry and
  reaction foundation is recorded in the [slice 758 task](native-engine-browser-758.md).
- The [HTML Standard custom-element algorithms](https://html.spec.whatwg.org/multipage/custom-elements.html)
  define customized built-ins by a registry name, a built-in local name, and
  the element's internal `is` value. The [DOM Standard creation algorithms](https://dom.spec.whatwg.org/#concept-create-element)
  define the `createElement()` options and compatibility string form.

## Objective

Implement customized built-in custom elements through the native global
registry. They retain the built-in local name and behavior while using the
author constructor/prototype and custom-element reactions. Support parser,
DOM-creation, constructor, mutation, serialization, cloning/import, and selected
same-origin frame-owner execution paths without CDP or fallback.

## Contract

- `customElements.define(name, constructor, { extends })` accepts a valid
  built-in HTML local name represented by the native element-interface table,
  rejects an autonomous custom name or unknown/unsupported base with
  `NotSupportedError`, and requires the author prototype to inherit from the
  matching built-in interface prototype. Definition publication remains
  atomic on invalid input.
- Definitions retain both the custom name and built-in local name. Lookup and
  upgrade require the HTML namespace, matching local name, matching internal
  `is` value, and the owning global registry. An autonomous definition must
  not upgrade an element whose internal `is` value is non-null, and a
  customized built-in must not upgrade a different built-in local name.
- The HTML interface constructor used by `super()` consumes the active
  construction entry only when its interface matches the definition. Direct
  `new CustomConstructor()` creates the underlying built-in element with
  stable native node identity, correct built-in prototype ancestry, internal
  custom name, and no reflected `is` content attribute.
- `document.createElement()` and `createElementNS()` accept the current DOM
  `ElementCreationOptions` dictionary and legacy string form. They preserve
  `is` as internal creation metadata, reject conflicting registry-plus-`is`
  options, and upgrade only when the selected global registry defines the
  matching customized built-in. `getAttribute("is")` remains null for
  programmatically created elements unless the page explicitly adds that
  content attribute; HTML serialization emits the internal `is` value when
  required without mutating the attribute list.
- Parser-created built-ins with `is` upgrade in tree order when their
  definition becomes available. Later `is` attribute changes do not rewrite
  an element's creation-time internal value. Insertion, explicit `upgrade()`,
  and registry definition use the existing bounded reaction queue and preserve
  constructor, attribute, connection, and exception ordering.
- Cloning and `importNode()` preserve the internal custom name independently
  of content attributes and use the destination's owning global registry.
  Selected same-origin frame execution uses its own registry and native
  element owner; it must not borrow the top-level registry.
- Existing built-in behavior continues to dispatch by the underlying local
  name. A customized button remains a button for native button operations,
  and its object remains an instance of both the custom constructor and the
  correct HTML interface.
- No scoped registry, form-associated custom-element/`ElementInternals`,
  custom-state, complete Web Platform Tests, or browser-completion claim is
  introduced by this slice; these remain issue #40 requirements.

## Tradeoffs

Customized built-ins preserve semantics and accessibility behavior already
attached to standard elements, which autonomous elements cannot recreate
reliably. Correctness depends on matching the registry definition, local name,
internal `is` value, and native interface constructor as one identity. Treating
`is` as an ordinary attribute would incorrectly expose programmatic creation,
allow later attribute mutation to change definition lookup, and lose the
serializer's distinction. This slice uses the existing supported native
interface table; unmapped HTML interfaces remain explicit gaps to close before
the full Core Web Profile gate, not silent aliases to `HTMLUnknownElement`.

## Paths

- `crates/glass-browser/src/browser/native_engine/dom.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-759.md`

## Verification

- `cargo fmt --all -- --check` passed.
- `cargo check -p glass-browser --lib --test native_engine --locked --quiet`
  passed; output contains existing dead-code warnings in `native_engine/dom.rs`.
- `cargo test -p glass-browser --test native_engine native_custom --locked --quiet`
  passed: 6 tests, 0 failures, 0 ignored (88.25 seconds).
- Process-backed regressions cover parser upgrade, object and string creation
  options, direct construction, correct interface identity, serialization
  without a reflected attribute, local-name and `is` mismatch, invalid base
  interfaces, mutation stability, cloning/import, reaction ordering, and
  selected same-origin frame execution.
- Documentation gates passed: release-truth (1,387 Markdown files; 0 stale
  current claims), depth (93 current guides), shortcut inventory (15 help keys,
  63 markers), and live coverage (1,387 Markdown files, 346 MCP tools, 17
  examples, 22 public modules).
- `git diff --check` passed.
- Remote CI, cross-platform runtime certification, unmapped HTML interfaces,
  full custom-element WPT, and issue #40 closure remain separate gates.
