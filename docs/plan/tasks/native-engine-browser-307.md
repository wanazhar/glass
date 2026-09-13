# Glass native engine browser slice 307: Web Crypto secret JWKs

Status: complete locally; issue #40 production parity and remote CI remain open.

## Objective

Complete the common secret-key serialization workflow with bounded JWK
`importKey()` and `exportKey()` for HMAC and AES-GCM.

## Contract

- `exportKey('jwk', key)` returns an RFC 7517-style `oct` JWK for extractable
  HMAC or AES-GCM keys, including the base64url key bytes, algorithm label,
  extractability flag, and key usages.
- `importKey('jwk', jwk, algorithm, extractable, keyUsages)` validates `kty`,
  bounded base64url `k`, optional `alg`, `ext`, and `key_ops` fields before
  materializing the existing opaque HMAC/AES-GCM key stores.
- HMAC accepts SHA-1/SHA-256/SHA-384/SHA-512 and maps to `HS1`/`HS256`/
  `HS384`/`HS512`; AES-GCM accepts 128/192/256-bit keys and maps to
  `A128GCM`/`A192GCM`/`A256GCM`.
- JWK import honors requested usages and rejects contradictory key operations,
  non-extractable JWK metadata requested as extractable, malformed/oversized
  material, unsupported formats, and algorithm/length mismatches before host
  work or key publication.
- Page and dedicated-worker realms use the same bounded base64url rules and
  retain key material outside enumerable properties and cross-realm transfer.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Tradeoffs and follow-up

JWK support is limited to symmetric `oct` keys so it can reuse the existing
RustCrypto-backed HMAC/AES-GCM operations and avoid pretending to support
asymmetric key security before its key-pair lifecycle exists. JWK/PKCS
asymmetric keys, richer JWK members, AES-CBC/CTR profiles, key transfer,
complete Web Crypto/Web IDL semantics, and the remaining issue #40 promotion
gates remain follow-up work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_page_and_worker_jwk_crypto_keys --locked -- --nocapture`
- `git diff --check`

The focused witness passes page and dedicated-worker JWK import/export for
HMAC and AES-GCM, base64url byte preservation, algorithm metadata, real
sign/verify and encrypt/decrypt operations, and typed extractability,
key-operation, algorithm, and key-type rejection. The evidence is local-only
until the wider issue #40 native-only gates pass; remote CI, publication,
release, and final native/CDP parity claims remain pending.
