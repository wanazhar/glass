# Glass native engine browser slice 302: Web Crypto HMAC keys

Status: complete locally; issue #40 production parity and remote CI remain open.

## Objective

Make the page and dedicated-worker Web Crypto surface useful for common
application signing and verification by adding bounded secret-key lifecycle
and HMAC operations without leaving native execution.

## Contract

- `CryptoKey` is an opaque, non-constructible secret-key object with stable
  `type`, `extractable`, `algorithm`, and `usages` projections.
- `crypto.subtle.importKey()` accepts bounded `raw` HMAC key material and
  SHA-1/SHA-256/SHA-384/SHA-512 hash descriptors; duplicate or unsupported
  usages reject explicitly.
- `exportKey('raw', key)` returns an `ArrayBuffer` only for extractable keys;
  non-extractable keys reject without exposing key bytes through the public
  object surface.
- `sign()` and `verify()` accept HMAC keys with matching `sign`/`verify`
  usages, bounded BufferSource data/signatures, and return Promise-backed
  `ArrayBuffer`/boolean results.
- Page and worker key stores survive native bootstrap re-entry but do not
  cross realms, frames, workers, structured-clone boundaries, or navigation.

## Path

- `crates/glass-browser/Cargo.toml`
- `Cargo.lock`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Tradeoffs and follow-up

RustCrypto HMAC implementations keep cryptographic processing out of the
generated JavaScript, while a short-lived host function bounds the copied
input and signature bytes. The realm-owned opaque key store preserves the
normal non-extractable key shape without adding a new IPC protocol. Key
generation, AES-GCM, HKDF/PBKDF2, RSA/EC/Ed25519, key transfer, and complete
Web IDL/error semantics remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_page_and_worker_expose_hmac_crypto_keys --locked -- --nocapture`
- `git diff --check`

The focused witness passes one page/worker test (all four HMAC known vectors,
raw export, usage/extractability rejection, stable key identity, and worker
signing). The evidence is local-only until the wider issue #40 native-only
gates pass; remote CI, publication, release, and final native/CDP parity
claims remain pending.
