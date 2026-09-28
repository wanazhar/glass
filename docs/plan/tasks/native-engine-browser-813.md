---
id: native-engine-browser-813
scope: glass-browser/shared-worker-constructor-matching
status: in-progress
depends-on: [native-engine-browser-812]
---

# Glass native-engine browser slice 813: SharedWorker constructor matching

## Objective

Make SharedWorker reuse follow the HTML constructor-matching algorithm. Reuse
only a live worker whose creator storage key, parsed constructor URL, and name
match. A matching identity with incompatible worker options must report an
error to the new `SharedWorker`; it must not silently start another global or
disturb the existing worker and connections.

## Context

- Issue [#40](https://github.com/wanazhar/glass/issues/40) owns the
  native-browser completion mission and closure gates.
- The versioned [Glass Core Web Profile](../native-engine-browser-profile.md)
  defines the required worker and origin behavior.
- [Slice 812](native-engine-browser-812.md) establishes live iframe Document
  teardown and owner-scoped SharedWorker connection retirement.
- The [HTML Standard SharedWorker constructor](https://html.spec.whatwg.org/multipage/workers.html#shared-workers-and-the-sharedworker-interface)
  matches constructor storage key, constructor URL, and name; `type`,
  `credentials`, and `extendedLifetime` mismatches queue an error and do not
  connect to the existing global.
- The [Storage Standard storage-key algorithms](https://storage.spec.whatwg.org/#storage-keys)
  currently define the non-storage key as an origin tuple. Opaque origins are
  unique identities; do not substitute the SharedWorker URL for the creator
  key.

## Contract

- Carry the creating environment storage key with each SharedWorker creation
  request. Tuple origins use their normalized serialized origin; opaque
  origins use a key scoped to the owning Document identity.
- Use `(creator storage key, parsed constructor URL, name)` as the registry
  match identity. Do not put `type`, `credentials`, or `extendedLifetime` in
  the identity key.
- For an existing live identity match, compare `type`, `credentials`, and
  `extendedLifetime`. On mismatch, queue exactly one error for the new
  `SharedWorker`, do not dispatch a worker `connect`, and leave the incumbent
  global, owner set, ports, and routes unchanged.
- A different storage key, URL, or name creates an independent worker global.
  A closed/removed worker is not eligible for matching.
- Apply constructor defaults: `type: "classic"`, `credentials:
  "same-origin"`, `name: ""`, and `extendedLifetime: false`. Reject invalid
  worker type and credentials enum values before command dispatch. The legacy
  string overload supplies only `name` and defaults the other options.
- Preserve bounded queue and ID behavior. A rejected connection must not leave
  a registered page route or transferred-port route behind.
- Credential-aware script/module fetching, the post-owner `extendedLifetime`
  timeout, between-loads retention, future storage partitioning, optional
  agent-cluster communication restrictions, and full SharedWorker/WPT
  conformance are separate issue #40 requirements.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/src/browser/native_engine/engine.rs`
- `crates/glass-browser/src/browser/native_backend.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/native-engine-browser-profile.md`
- `docs/plan/tasks/native-engine-browser-813.md`

## Verification

- Run `cargo check -p glass-browser --test native_engine --locked --quiet`
  after the implementation batch.
- Add a focused real-HTTP process-backed regression for identity reuse,
  type/credentials/extendedLifetime mismatch errors, absence of a second
  `connect`, and incumbent route survival. Cover distinct name/URL workers.
- Add deterministic coverage that distinct opaque Document identities do not
  share a storage key and that equal tuple origins do.
- Verify rejected connections do not leak routes or bridge ownership; then
  rerun the focused regression after any correction.
- Run `cargo fmt --all -- --check`, `git diff --check`, and the maintainer
  release-documentation truth, depth, TUI-shortcut, and coverage gates. Record
  exact commands, warnings, elapsed time, and remaining boundaries.
- Keep this checkpoint local; do not push or run remote CI.

## Results

Implementation and verification are in progress.
