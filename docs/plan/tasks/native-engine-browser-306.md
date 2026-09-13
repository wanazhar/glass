# Glass native engine browser slice 306: Web Crypto key wrapping

Status: complete locally; issue #40 production parity and remote CI remain open.

## Objective

Close the common secret-key transport workflow by adding bounded raw-key
`wrapKey()` and `unwrapKey()` over the existing realm-local AES-GCM owner.

## Contract

- `wrapKey()` accepts the raw format, an extractable realm-local HMAC or
  AES-GCM secret key, and a valid AES-GCM wrapping key with `encrypt` usage.
- `unwrapKey()` accepts a bounded raw wrapped payload, a valid AES-GCM
  unwrapping key with `decrypt` usage, and restores an HMAC or AES-GCM key
  under the existing opaque key stores and usage/length contracts.
- AES-GCM wrapping retains the existing 12-byte IV, optional bounded AAD, and
  128-bit authentication-tag policy; authentication failures reject as
  `OperationError` without publishing a key.
- Wrapped bytes and restored key material remain realm-local, non-enumerable,
  and excluded from structured-clone/page-worker transfer paths. Non-extractable
  source keys, wrong key kinds/usages, unsupported formats, invalid target
  lengths, and oversized payloads fail before mutation or host work.
- Page and dedicated-worker implementations share the existing captured host
  AES-GCM source rather than introducing a second transport or crypto runtime.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Tradeoffs and follow-up

Restricting the first wrapping contract to raw secret keys and AES-GCM keeps
the operation interoperable with the already-tested RustCrypto owner while
avoiding a new serialization or asymmetric-key dependency. JWK/PKCS import and
export, AES-CBC/CTR profiles, asymmetric keys, key transfer, complete Web
Crypto/Web IDL semantics, and the remaining issue #40 promotion gates remain
follow-up work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_page_and_worker_wrap_crypto_keys --locked -- --nocapture`
- `git diff --check`

The focused witness passes page and dedicated-worker wrapping and unwrapping
of HMAC and AES-GCM keys, raw-byte restoration, real sign/verify and
encrypt/decrypt operations, authentication-tamper rejection, and typed
non-extractable, usage, and format rejection. The evidence is local-only until
the wider issue #40 native-only gates pass; remote CI, publication, release,
and final native/CDP parity claims remain pending.
