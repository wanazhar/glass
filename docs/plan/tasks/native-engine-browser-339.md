# Native SharedWorker ownership and connections (339)

Status: implemented locally in the current 0.3.14 checkout.

This slice adds the native SharedWorker ownership contract to both native
execution paths:

- `SharedWorker(url, { name, type })` validates bounded names, worker types,
  same-origin HTTP(S)/fixture URLs, and the transferred connection endpoint;
- matching URL/name/type keys reuse one isolated worker runtime, while each
  constructor receives a distinct `MessagePort` connection;
- the worker receives a bounded `connect` event through the existing
  cross-realm bridge, can retain connection state, and can send replies back
  through each port;
- classic and module worker sources use the existing worker loader, static
  dependency graph, MIME, URL, and byte policies; and
- initial inline page commands are drained before the next local script turn,
  so local fixture startup and HTTP(S) content-process startup both deliver
  connection messages without a hidden CDP or fallback path.

## Tradeoffs

- The shared-worker key is the resolved worker URL, constructor name, and
  normalized worker type. This prevents classic and module connections from
  accidentally sharing incompatible global setup without adding a third
  runtime or a separate process pool.
- Each connection is represented by one bounded bridge descriptor. Rust owns
  the route and QuickJS owns only the receiving endpoint, so JavaScript
  objects, pointers, and unvalidated realm state never cross the host
  boundary.
- Message delivery remains a bounded page-turn queue. It is deterministic and
  auditable across local and content-process owners, while complete browser
  task-source interleaving and richer transferable values remain separate
  profile work.
- The runtime is retained while its named SharedWorker remains owned by the
  page; `MessagePort.close()` removes endpoint delivery but does not invent
  premature worker termination. Durable worker script/cache persistence is
  still part of the profile/storage promotion work.

## Local evidence

- `cargo fmt --all`
- `git diff --check`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo check --quiet -p glass-browser --test native_engine --locked`
- `CARGO_PROFILE_TEST_DEBUG=0 cargo test --quiet -p glass-browser --test native_engine shared_worker --locked -- --nocapture` — 2 passed

The two integration witnesses cover initial inline creation, named-runtime
reuse, independent connection ports, ready/reply delivery, and local fixture
plus HTTP(S) content-process ownership. All evidence is local; this checkout
has not been pushed and has no remote CI result.
