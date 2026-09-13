# Glass native engine browser slice 308: Web Crypto AES-CBC and AES-CTR

Status: complete locally.

## Objective

Close the common symmetric encryption gap by adding bounded AES-CBC and
AES-CTR to the native Web Crypto secret-key surface.

## Contract

- Page and dedicated-worker realms accept AES-CBC and AES-CTR 128/192/256-bit
  secret keys through raw import and key generation, preserving the existing
  opaque realm-local `CryptoKey` lifecycle and encrypt/decrypt usages.
- AES-CBC uses a required 16-byte IV and standard Web Crypto PKCS#7 padding;
  malformed block lengths or padding reject without publishing plaintext.
- AES-CTR uses a required 16-byte counter and a validated 1..128-bit
  `length`, incrementing only the declared low-order counter bits.
- Input/output sizes remain bounded by the existing native form-body limit;
  Rust owns the AES block-mode operation and page/worker host helpers are
  captured per turn and removed before user code runs.
- Existing AES-GCM behavior remains unchanged. CBC/CTR keys cannot silently
  enter the GCM-only JWK or wrapping paths until their serialization and
  authenticated-wrapping contracts are separately defined.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Tradeoffs and follow-up

Implementing CBC/CTR over the already-vendored AES block primitive avoids a
new mode dependency and keeps build growth small, but the mode helpers must
maintain their own padding and counter invariants. CBC provides confidentiality
but not authentication, so it is intentionally not accepted as a wrapping
algorithm. AEAD profiles, richer Web Crypto/Web IDL semantics, asymmetric
keys, and the remaining issue #40 promotion gates remain follow-up work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_page_and_worker_aes_block_crypto --locked -- --nocapture`
- `git diff --check`

The focused integration test and the isolated Rust NIST-vector test both pass
locally. The regression filter covering the eight page/worker crypto witnesses
also passes; no remote CI, publication, release, or native/CDP parity claim is
made by this slice alone.

The evidence is local-only until the wider issue #40 native-only gates pass;
remote CI, publication, release, and final native/CDP parity claims remain
pending.
