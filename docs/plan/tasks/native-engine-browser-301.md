# Glass native engine browser slice 301: Web Crypto digests

Status: completed locally.

## Objective

Add the first useful `SubtleCrypto` operation to page and dedicated-worker
realms so ordinary applications can verify content without leaving native
execution or using an ad-hoc JavaScript hash implementation.

## Contract

- `crypto.subtle.digest()` accepts a string algorithm or an object with a
  `name` field and returns a Promise for an `ArrayBuffer`.
- SHA-1, SHA-256, SHA-384, and SHA-512 are computed by Rust's audited digest
  implementations; algorithm names are normalized case-insensitively.
- Page and worker BufferSource inputs accept `ArrayBuffer` and typed-array/
  `DataView` views while preserving byte offsets and lengths.
- Inputs remain under the native body-size bound; unsupported algorithms,
  invalid inputs, oversized data, and missing host capability reject
  explicitly with typed errors.
- Page and worker `crypto.subtle` objects retain stable identity across native
  bootstrap re-entry, and the digest host function is captured then removed
  from the public global surface.

## Path

- `crates/glass-browser/Cargo.toml`
- `Cargo.lock`
- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

The native runtime installs a short-lived QuickJS function that decodes a
bounded base64 BufferSource, dispatches to `sha1`/`sha2`, and returns a bounded
base64 digest. Page and worker bootstrap closures normalize the Web Crypto
algorithm input, encode/decode the bytes through their existing helpers, and
resolve or reject a Promise without adding another transport command.

## Tradeoffs and follow-up

Using Rust digest implementations avoids duplicating cryptographic code in
the generated JavaScript, but the synchronous host call still executes on the
realm turn and is bounded by the native body limit. SHA-1 remains available
for Web Crypto compatibility but must not be selected for new security
protocols. `CryptoKey`, import/export, HMAC/RSA/EC/Ed25519 signing and
verification, AES encryption, key derivation, and full Web IDL semantics remain
issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_page_and_worker_expose_subtle_digest --locked -- --nocapture`
- `git diff --check`

The local compile and witnesses pass. Remote CI, registry publication,
release, and final native/CDP parity or production-promotion claims remain
pending the wider issue #40 gates.
