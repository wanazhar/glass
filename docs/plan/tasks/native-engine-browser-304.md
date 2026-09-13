# Glass native engine browser slice 304: Web Crypto key generation

Status: complete locally; issue #40 production parity and remote CI remain open.

## Objective

Complete the common native Web Crypto key lifecycle by generating realm-owned
HMAC and AES-GCM secret keys from the existing OS-seeded crypto source instead
of requiring callers to manufacture raw key bytes.

## Contract

- `crypto.subtle.generateKey()` accepts HMAC algorithms using
  SHA-1/SHA-256/SHA-384/SHA-512 and AES-GCM algorithms using 128/192/256-bit
  keys, returning a Promise-backed opaque `CryptoKey` in page and dedicated
  worker realms.
- HMAC generation uses the declared bit length when supplied and the
  hash-specific block-size default otherwise; lengths are positive, byte
  aligned, and bounded before allocation.
- AES-GCM generation accepts only 128, 192, or 256 bits. Generated keys use
  the same encrypt/decrypt, export, sign, and verify contracts as imported
  keys, including extractability and usage validation.
- Key bytes come from the existing bounded OS-backed realm crypto pool and do
  not enter enumerable key properties, logs, structured-clone payloads, or
  another realm.
- Generation preserves stable constructor/key identity across native
  bootstrap re-entry and fails with typed errors for unsupported algorithms,
  invalid lengths, and invalid usages.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Tradeoffs and follow-up

Reusing the existing OS-backed pool avoids a new host callback and keeps key
generation inside the current realm quota and replenishment model. This slice
generates secret keys only; key wrapping, deriveBits/deriveKey, asymmetric
key pairs, JWK/PKCS8/SPKI import/export, transfer, and complete Web
Crypto/Web IDL semantics remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_page_and_worker_generate_crypto_keys --locked -- --nocapture`
- `git diff --check`

The focused witness passes one page/worker test covering generated HMAC and
AES keys, default and explicit lengths, real sign/encrypt/decrypt operations,
stable identity, and typed invalid-length/usage/algorithm errors. The evidence
is local-only until the wider issue #40 native-only gates pass; remote CI,
publication, release, and final native/CDP parity claims remain pending.
