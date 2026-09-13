# Glass native engine browser slice 309: Ed25519 Web Crypto keys

Status: completed locally; issue #40 production parity and remote CI remain
open.

## Objective

Add a useful asymmetric Web Crypto path to page and dedicated-worker realms so
native applications can sign and verify data without leaving the native
execution path.

## Contract

- `crypto.subtle.generateKey()` accepts `Ed25519` and returns an opaque,
  realm-local `{ publicKey, privateKey }` pair with the standard public/private
  key projections and usage partitioning.
- `crypto.subtle.importKey('raw', ...)` accepts a validated 32-byte Ed25519
  public key with `verify` usage.
- `crypto.subtle.importKey('jwk', ...)` and `exportKey('jwk', ...)` support
  bounded OKP/Ed25519 public and private JWKs with `x`, optional `d`, `EdDSA`,
  `ext`, and `key_ops` consistency checks.
- `sign()` and `verify()` use the Rust Ed25519 implementation, preserve empty
  and bounded message inputs, and reject wrong algorithms, key kinds, usages,
  malformed signatures, and invalid public points with typed errors.
- Public raw export is available only for extractable public keys. Private
  PKCS#8/SPKI formats, RSA, ECDSA/ECDH, and cross-realm key transfer remain
  separate capabilities and must not be implied by this slice.
- Page and worker keys remain outside enumerable properties and do not cross
  frames, workers, structured-clone boundaries, or navigation.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

The existing per-turn native crypto source now owns Ed25519 public-key
derivation, point validation, signing, and verification. Page and worker
bootstraps capture that source before removing it from the public global
surface. Each realm stores only opaque key state in its existing `WeakMap`,
retaining the private seed separately from its derived public bytes for JWK
export and pair consistency checks.

## Tradeoffs and follow-up

Reusing `ed25519-dalek` avoids another cryptographic dependency and keeps the
build profile stable. The source boundary bounds copied messages and validates
public points before publication, while the generated JavaScript remains a
thin Web Crypto projection. This deliberately covers one modern asymmetric
algorithm first; RSA/EC algorithms, PKCS#8/SPKI codecs, complete Web Crypto
algorithm dictionaries, transfer semantics, and full Web IDL descriptors
remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --lib native_ed25519_tests --locked -- --nocapture`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_page_and_worker_ed25519_crypto_keys --locked -- --nocapture`
- `git diff --check`

The local RFC 8032 vector, page/worker generated-pair round trip, JWK/raw
serialization, tamper rejection, and usage/mismatch witnesses pass. Remote CI,
publication, release, and final native/CDP parity or production-promotion
claims remain pending the wider issue #40 gates.
