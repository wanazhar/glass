# Native engine browser slice 246: semantic pointer actions

Status: completed locally.

## Objective

Make the shared pointer action surface executable by the native backend for
local documents, HTTP(S) content workers, and nested frame routes. The native
engine must accept the same high-level `doubleClick`, `hover`, and `drag`
actions already exposed by Glass CLI, MCP, and the Chromium session API.

## Contract

- `SemanticAction` and `NativeAction` carry bounded double-click, hover, and
  source-to-destination drag requests.
- Native target validation rejects detached, hidden, zero-layout, and
  `pointer-events:none` targets before mutation or event delivery.
- Double-click produces two ordinary click transactions, preserving the
  existing focus, default-action, navigation, popup, and revision owners.
- Hover emits `mouseover` followed by non-bubbling `mouseenter`.
- Drag emits `dragstart`, `dragenter`, `dragover`, `drop`, and `dragend` in
  source/destination order with browser-compatible bubbling and cancellation
  metadata.
- The content worker uses the same typed event bridge as form actions, so
  JavaScript listeners and their DOM mutations remain owned by the child
  realm before the parent commits the resulting document snapshot.
- Child-frame semantic locators resolve both drag endpoints in one frame and
  dispatch through the existing frame/effect coordinator.
- CLI and MCP native dispatches use the shared action contract; Chromium keeps
  its established pointer implementation, while BiDi/WebDriver/proof
  adapters reject the newly explicit actions with their existing capability
  errors.

## Implementation

The native DOM now owns pointer event metadata and validates both endpoints
against the attached layout and computed pointer-event state. The engine
routes local actions through its existing event transaction and routes
HTTP(S) actions through `mutate_form_events` in the content worker. The worker
serializes the resulting document, event effects, script history, scroll
commands, and browser effects using the established mutation response.

The native backend maps all three actions in root and parked child frames.
Drag resolves source and destination together, preventing a target in one
frame from being paired with a target in another. The CLI and native MCP
paths now reach the same runtime action method instead of rejecting these
commands as an incomplete native slice.

## Tradeoffs and follow-up

Double-click intentionally reuses two click transactions, which keeps link,
form, popup, and cancellation behavior consistent but means a navigation from
the first click can detach the second target, matching the observable browser
lifecycle. The current semantic drag contract does not synthesize coordinates
or expose a `DataTransfer` object; it establishes DOM event ordering and
listener effects first. Pointer hover state, `:hover` style invalidation,
pointer coordinates, drag payloads, and native pointer capture remain later
browser-profile work rather than being silently faked here.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --features native-engine`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine pointer_actions -- --nocapture` (2 passed, 0 failed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_backend_ -- --nocapture` (7 passed, 0 failed)
- `git diff --check`

Remote CI, push, release, tag, registry publication, and production-parity
claims are not made by this local checkpoint.
