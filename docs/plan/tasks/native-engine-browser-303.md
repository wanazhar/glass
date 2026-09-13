# Glass native engine browser slice 303: Web Crypto AES-GCM

Status: complete locally; issue #40 production parity and remote CI remain open.

## Objective

Add the authenticated-encryption path ordinary browser applications use after
key import: bounded raw AES-GCM keys and Promise-backed page/worker
`encrypt()`/`decrypt()` operations owned by the native Web Crypto bridge.

## Contract

- `crypto.subtle.importKey()` accepts bounded `raw` AES-GCM keys of 128, 192,
  or 256 bits and exposes an opaque realm-local `CryptoKey` with stable
  `type`, `extractable`, `algorithm`, and `usages` projections.
- `crypto.subtle.exportKey('raw', key)` returns the original bytes only for
  extractable AES keys; non-extractable keys remain opaque.
- `encrypt()` and `decrypt()` accept an AES-GCM algorithm object with a
  bounded 12-byte IV, optional bounded `additionalData`, and the default
  128-bit authentication tag. Ciphertext carries the authentication tag in
  the Web Crypto order (ciphertext followed by tag).
- AES operations accept bounded ArrayBuffer/view input, preserve view ranges,
  reject mismatched key algorithms/usages, and return Promise-backed
  ArrayBuffers or typed operation errors.
- Page and dedicated-worker AES keys remain realm-owned across bootstrap
  re-entry and do not cross frames, workers, structured-clone boundaries, or
  navigation.

## Path

- `crates/glass-browser/Cargo.toml`
- `Cargo.lock`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Tradeoffs and follow-up

RustCrypto AES-GCM keeps authenticated encryption out of generated
JavaScript, while a short-lived host function bounds copied key, IV, AAD, and
payload bytes. The first contract uses the interoperable 12-byte IV and
128-bit tag path and does not accept unsupported AES usages as if wrap/unwrap
operations existed. Variable tag lengths, arbitrary IV lengths, AES-CBC/CTR,
key generation, key wrapping, HKDF/PBKDF2, asymmetric keys, key transfer, and
complete Web Crypto/Web IDL semantics remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_page_and_worker_expose_aes_gcm_crypto_keys --locked -- --nocapture`
- `git diff --check`

The focused witness passes one page/worker test covering AES-128/192/256,
the NIST AES-128-GCM vector, AAD, view ranges, decrypt round trips, tamper
rejection, key identity, and typed parameter/usage errors. The evidence is
local-only until the wider issue #40 native-only gates pass; remote CI,
publication, release, and final native/CDP parity claims remain pending.
