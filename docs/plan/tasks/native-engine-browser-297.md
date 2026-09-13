# Glass native engine browser slice 297: worker cryptography

Status: completed locally.

## Objective

Provide the secure random primitives ordinary worker libraries use without
using `Math.random()` as a cryptographic substitute or creating a third host
boundary.

## Contract

- Each worker realm receives a fresh bounded random seed from Rust's
  operating-system-backed `getrandom` source.
- `crypto.getRandomValues()` accepts integer typed arrays, writes bounded
  random bytes in place, returns the original array, and rejects floating
  point arrays, invalid views, and values over the explicit quota.
- `crypto.randomUUID()` returns a bounded UUID string with version 4 and the
  RFC 4122 variant bits set.
- Random pool exhaustion and invalid requests fail explicitly with typed
  errors; no deterministic or `Math.random()` fallback is used.
- The crypto object and its methods remain available across re-entrant worker
  bootstrap turns without exposing the random pool to the page realm.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

`worker_bootstrap()` requests a bounded OS-random seed through `getrandom`,
serializes it into the isolated realm, and appends it to a bounded worker
pool on host re-entry. The worker `crypto` surface validates integer typed
array tags, enforces the byte quota, writes through a byte view, and formats
UUID v4 values after setting the version and variant bits. The realm keeps
the crypto object and consumed-offset state across message, timer, and
network turns.

The local witness checks random-array sizing, UUID format/version/variant,
floating-point rejection, and crypto object identity. It does not assert a
particular random value.

## Tradeoffs and follow-up

OS seeding keeps the security property explicit and avoids an insecure random
fallback, while the fixed pool bounds memory and keeps JavaScript execution
deterministic from the host's perspective. A worker that exhausts the pool
must wait for another host turn or receive an explicit quota error. Web Crypto
`subtle` operations, `CryptoKey` identity, key import/export, cross-realm key
transfer, and complete Web IDL semantics remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_worker_exposes_os_seeded_crypto --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine worker --locked` (21 passed)
- `git diff --check`

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider Issue #40 gates.
