# Native service-worker ownership and interception (336)

Status: implemented locally in the current 0.3.14 checkout.

This slice gives the native content process a real service-worker owner for
ordinary HTTP(S) pages:

- `navigator.serviceWorker.register()` validates same-origin HTTP(S) script
  and scope URLs, supports classic and static module worker sources, loads the
  source through the existing MIME, byte, URL, and module-graph policy, and
  resolves an activated `ServiceWorkerRegistration` after install and
  activate lifecycle dispatch;
- the content process keeps registrations alive across navigations, exposes
  bounded `getRegistration()`, `getRegistrations()`, `ready`, `controller`,
  `ServiceWorkerRegistration.active`, and `unregister()` state to each page,
  and selects the longest matching same-origin scope;
- navigation and page Fetch requests enter the worker's `fetch` event with
  bounded request metadata/body bytes; `respondWith(new Response(...))`
  returns validated status, headers, content type, URL, and byte-preserving
  body data through the owner, while an unhandled request continues through
  the ordinary native loader;
- lifecycle and fetch scripts execute in an isolated QuickJS worker realm and
  all network access remains owned by the existing native resource loader;
  no CDP process or hidden browser fallback is introduced; and
- page-to-service-worker `postMessage()` is bounded and owner-routed. The
  cross-context `MessagePort` transfer and worker-to-client event channel are
  separate follow-up browser surfaces, not silently represented by a same-realm
  shortcut.

## Tradeoffs

- Registration state is process-owned and bounded to sixteen scopes. This
  keeps reload and interception deterministic without inventing a second
  profile database; durable service-worker script/cache persistence still
  belongs to the storage/profile promotion work.
- Request and response bodies cross the realm boundary as bounded JSON/base64
  envelopes. This preserves bytes and origin ownership while keeping the
  content process auditable; transport-demand streaming remains a separate
  optimization.
- The worker uses the existing QuickJS and native loader owners instead of
  adding another runtime. That keeps the two-crate workspace and build graph
  stable, at the cost of implementing additional Web IDL and task-source
  surfaces incrementally.

## Local evidence

- `cargo fmt --all`
- `git diff --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_content_process_registers_service_worker_and_intercepts_fetch_and_navigation --locked -- --nocapture` — passed

The integration fixture proves registration/activation, worker-generated
navigation, worker-generated Fetch response metadata/body, and network
fallback after unregister. All evidence is local; this checkout has not been
pushed and has no remote CI result.
