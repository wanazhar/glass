# Glass native engine browser slice 298: page cryptography

Status: completed locally.

## Objective

Expose the same bounded OS-seeded random primitives to the ordinary document
realm that slice 297 added to dedicated workers. Page code must be able to
create UUIDs and fill integer typed arrays without relying on `Math.random()`
or a second transport boundary.

## Contract

- Each page bootstrap receives fresh operating-system-backed seed bytes from
  Rust's `getrandom` source.
- `crypto.getRandomValues()` accepts integer typed arrays, writes into the
  original view, returns that view, and rejects floating-point arrays and
  values above the explicit byte quota.
- `crypto.randomUUID()` returns a version-4 UUID with RFC variant bits.
- The page crypto object and consumed pool state survive re-entrant document
  bootstrap turns, including script evaluation and host event delivery.
- Exhaustion and invalid requests are explicit errors; there is no
  deterministic or `Math.random()` fallback.
- The existing cross-origin WindowProxy guard continues to treat `crypto` as
  same-origin-only state.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

`document_bootstrap()` seeds a bounded page pool from the operating system and
installs a persistent page `crypto` object alongside the document navigator.
The implementation mirrors the worker validation and UUID bit rules while
preserving any existing realm object identity. The local witness verifies
array writes, UUID format/version/variant, invalid typed-array rejection,
quota rejection, and global identity.

## Tradeoffs and follow-up

Keeping the page and worker surfaces bounded makes the current synchronous
host boundary auditable and prevents unbounded memory growth. The next Web
Crypto work remains `subtle`, `CryptoKey` identity, key import/export,
digests, signing, encryption, and cross-realm key semantics; those are not
silently represented by the two random primitives. Pool replenishment and
full Web IDL descriptor parity are also issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_page_exposes_os_seeded_crypto --locked -- --nocapture`
- `git diff --check`

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider issue #40 gates.
