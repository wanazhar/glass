# Native engine browser-complete slice 441: XHR EventTarget identity

status: complete
scope: native-engine/xhr-event-target-identity
issue: 40
depends-on: [native-engine-browser-440]

## Objective

Close the XHR Web IDL inheritance gap without adding another event-listener
implementation. Page and dedicated/SharedWorker `XMLHttpRequest` objects and
their upload targets must participate in the realm's existing `EventTarget`
contract while retaining the bounded native transport and lifecycle owner.

## Contract

- Page and worker `XMLHttpRequest.prototype` and `XMLHttpRequestUpload.prototype`
  inherit from their realm's `EventTarget.prototype`.
- XHR and upload instances satisfy both their specific constructor identity and
  `instanceof EventTarget`; `addEventListener`, `removeEventListener`, and
  `dispatchEvent` are inherited rather than parallel XHR-only methods.
- XHR event-handler attributes (`onreadystatechange`, `onload`, upload
  progress/terminal handlers, and the other bounded XHR event names) use the
  same EventTarget listener owner as explicit listeners. Synthetic dispatch
  invokes the handler and listener in registration order, and native lifecycle
  dispatch keeps its existing target, progress, cancellation, and terminal
  behavior.
- Event listener exceptions remain isolated from the XHR transport turn. The
  existing bounded event/listener limits and realm ownership rules remain in
  force.

## Tradeoffs

- Sharing the realm EventTarget store gives XHR the correct inherited API and
  listener options/identity, but XHR listeners now consume the same bounded
  listener budget as other page targets.
- The native XHR event set remains explicitly bounded; arbitrary `on<name>`
  properties are not manufactured. XHR events remain non-bubbling because an
  XHR has no DOM parent path.
- Worker XHR keeps its compact worker EventTarget store, while page XHR uses
  the page's owner-backed store. This preserves cross-realm isolation instead
  of pretending that page and worker objects share identity.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_xhr_exposes_status_text_and_safe_response_headers --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine xhr --locked -- --test-threads=1 --nocapture` (21 passed, 0 failed)
- `git diff --check`

The content-process witness covers page and worker prototype inheritance,
constructor/`instanceof` identity, inherited listener methods, synthetic
`dispatchEvent`, and the existing HTTP response-header/status-text contract.
