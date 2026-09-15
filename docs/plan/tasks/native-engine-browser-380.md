# Native host event structured transport (380)

```yaml
id: native-engine-browser-380
scope: native-engine/host-event-transport
status: done
depends-on:
  - native-engine-browser-379
```

## Objective

Remove generated JavaScript source from ordinary browser-owned event delivery.
Focus, click, submit, keyboard, form, lifecycle, validation, image, and scroll
events must cross local and content-process page owners as bounded structured
metadata while preserving event ordering and default-prevention results.

## Delivered behavior

- `NativeHostEvent` records carry the node target, event flags, keyboard
  metadata, and form submitter identity without embedding values in source.
- The installed `__glassDispatchHostEvents` function receives parsed structured
  data and stores its per-event boolean results for the static continuation.
- Local and content-process click, key, submit, lifecycle, form, validation,
  image, and scroll paths now use the same typed event bridge.
- Click, submit, keydown, and beforeunload callers retain the existing
  `defaultPrevented`/allowed result contract.
- Supported event types, keyboard values, modifier masks, submitter identity,
  event counts, JSON envelopes, and host-turn limits remain validated before
  dispatch.
- The previous ordinary host-event source builders and all of their call sites
  are removed.

## Contract and tradeoffs

The page owner pays one bounded JSON parse for each host event turn, but event
metadata no longer consumes the authored JavaScript source budget or shares an
executable string with page code. The dispatcher remains installed JavaScript
because it owns DOM event-path semantics; Rust owns the event record, target
identity, validation, and result boundary. This slice preserves the existing
per-turn queue order and does not claim one browser-wide scheduler across every
task source. Full Core Web Profile conformance, recovery/cancellation,
cross-platform certification, and production release evidence remain open
issue #40 gates.

## Touched paths

- `crates/glass-browser/src/browser/native_engine/content_process.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/plan/tasks/native-engine-browser-380.md`

## Verification

The following checks passed locally against this slice:

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --lib --locked`
- `cargo test -p glass-browser --lib native_host_event_tests::host_key_event_batch_dispatches_quoted_metadata_without_source_interpolation --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_local_script_owns_event_listeners_and_focus_order --locked -- --nocapture` — passed
- `cargo test --quiet -p glass-browser --test native_engine native_local_keypress_edits_focused_text_and_honors_keydown_cancel --locked -- --nocapture` — passed
- `cargo test --quiet -p glass-browser --test native_engine native_local_form_submit_event_can_cancel_request_submit --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_pointer_actions_bridge_dom_events --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_keyboard_actions_preserve_event_and_modifier_contract --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_orders_navigation_lifecycle_events --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_script_form_submit_navigates_with_get_controls --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_form_validation_blocks_submit --locked -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --test native_engine native_local_page_lifecycle_events_fire_after_script_schedule --locked -- --nocapture` — 1 passed
- `git diff --check`

Remote CI, push, release, tag, and registry publication evidence are not part
of this local-only checkpoint.
