# Native `script-src-attr` enforcement (396)

```yaml
id: native-engine-browser-396
scope: native-engine/inline-event-handler-csp
status: done
depends-on:
  - native-engine-browser-395
```

## Objective

Close the remaining inline-event-handler CSP bypass in the HTTP(S) content
owner. Event-handler content attributes such as `onclick="..."` must use the
same Rust-owned CSP decision boundary as inline script elements.

## Contract

- Preserve `script-src-attr` source-expression bytes and apply its fallback
  chain to `script-src`, then `default-src`.
- Admit an `on*` content attribute only for `unsafe-inline` or a matching
  supported hash policy form; attributes have no nonce channel, so a
  nonce-only policy must block them.
- Compile admitted handlers in the page realm with the element as `this` and
  the dispatched event as the `event` argument. A returned `false` must cancel
  a cancelable event.
- Reconcile handlers when attributes are created, replaced, removed, or
  refreshed by a host document projection. Blocked handlers must never enter
  the listener registry or execute.
- Keep ordinary `addEventListener`, event-handler IDL properties already owned
  by resource objects, local fixture/data documents, and existing event
  propagation behavior unchanged.
- Preserve source, URL, process, listener-count, script, and command bounds;
  no policy-bearing source is logged or sent through a new transport.

## Explicit follow-up

This slice does not claim CSP meta-policy parsing, report-only policy,
`SecurityPolicyViolationEvent` delivery, strict-dynamic trust propagation,
full CSP source grammar, or complete event-handler Web IDL descriptor parity.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/resource_loader.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-396.md`

## Tradeoffs

The handler compiler stays in the existing QuickJS page realm, while the
admission decision stays in the Rust resource loader. This avoids a second
event system and preserves the current bounded event path, but it intentionally
does not emulate the full browser-specific `with` environment or every event
handler property until the broader Web IDL slice owns those descriptors.

## Delivered

- Added `script-src-attr` to the Rust CSP policy with the
  `script-src`/`default-src` fallback chain and preserved source-expression
  bytes for hash matching.
- Required `unsafe-hashes` for hash-admitted inline event-handler attributes;
  nonce-only policies cannot admit an attribute without a nonce channel.
- Installed admitted `on*` content attributes as bounded native event
  listeners with element `this`, event argument, and `return false`
  cancellation behavior.
- Reconciled handler listeners across initial snapshots, `setAttribute`,
  `removeAttribute`, host refreshes, and dynamic element creation without
  leaving stale listeners under remapped node identities.
- Kept ordinary listener registration and resource-owned event-handler
  properties on the existing event path.

## Verification

- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo test --quiet -p glass-browser --lib csp_inline_sources_preserve_nonce_hash_and_element_directive_semantics --locked -- --nocapture` — passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_enforces_script_src_attr_for_initial_and_dynamic_handlers --locked -- --nocapture` — passed
- `cargo test --quiet -p glass-browser --test native_engine native_local_script_owns_event_listeners_and_focus_order --locked -- --nocapture` — passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_enforces_inline_csp_for_initial_and_dynamic_content --locked -- --nocapture` — passed
- `cargo fmt --all -- --check`
- `git diff --check`
- Release-documentation, documentation-depth, TUI-shortcut, and documentation-coverage validators

Remote CI, push, release, tag, and registry publication remain outside this
local-only checkpoint.
