# Glass native engine browser slice 305: Web Crypto key derivation

Status: complete locally; issue #40 production parity and remote CI remain open.

## Objective

Close the common secret-derivation workflow by adding bounded HKDF and PBKDF2
base keys, derived bits, and derived HMAC/AES-GCM keys in page and dedicated
worker realms.

## Contract

- `crypto.subtle.importKey()` accepts bounded raw HKDF and PBKDF2 base-key
  material as opaque, non-extractable realm-local `CryptoKey` values.
- `deriveBits()` accepts HKDF SHA-1/SHA-256/SHA-384/SHA-512 descriptors with
  bounded `salt`/`info`, and PBKDF2 descriptors with bounded salt and positive
  iteration counts; output lengths are byte-aligned and bounded.
- HKDF follows RFC 5869 extract/expand semantics, including the empty-salt
  rule; PBKDF2 follows the standard block/XOR construction with the same
  SHA-family hashes.
- `deriveKey()` materializes derived HMAC or AES-GCM keys using the existing
  opaque key stores and the target algorithm's length/usages contract.
- Derivation keys accept only `deriveBits`/`deriveKey`, remain non-extractable,
  and never expose base or derived bytes through enumerable properties, logs,
  structured clone, another realm, or navigation.
- Iteration, output, input, and total-work limits reject explicitly with typed
  errors before unbounded allocation or CPU work.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Tradeoffs and follow-up

The implementation reuses the existing RustCrypto HMAC family and the single
captured per-turn host boundary, avoiding another crypto dependency or a
second transport protocol. Bounded PBKDF2 work is deliberate: callers needing
larger password-hardening budgets must perform them outside a browser turn.
The host and realm layers also account for input-byte work before deriving, so
large salts/info values cannot multiply across blocks without a typed rejection.
AES-CBC/CTR, variable AES-GCM IV/tag profiles, key wrapping, JWK/PKCS import
and export, asymmetric keys, transfer, and complete Web Crypto/Web IDL
semantics remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_page_and_worker_derive_crypto_keys --locked -- --nocapture`
- `git diff --check`

The focused witness passes the RFC 5869 HKDF vector and RFC 6070 PBKDF2
vectors in a page realm and the HKDF vector in a dedicated worker. It also
proves derived AES-GCM encryption/decryption, derived HMAC signing/verification,
realm-local non-extractability, and typed mismatch/length/iteration rejection.
The evidence is local-only until the wider issue #40 native-only gates pass;
remote CI, publication, release, and final native/CDP parity claims remain
pending.
