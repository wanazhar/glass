# Glass native engine browser slice 300: crypto pool replenishment

Status: completed locally.

## Objective

Make the native random primitives usable across repeated calls without an
artificial exhaustion after the bootstrap seed is consumed, while keeping each
synchronous request bounded and OS-backed.

## Contract

- Page and dedicated-worker realms retain a small bounded fast-path pool.
- When the pool cannot satisfy a request, the realm calls a host-installed,
  non-page-owned OS random source for only the missing bytes.
- The host source is installed for the current native realm turn, captured by
  the bootstrap closure, and removed from the public global surface before
  page/worker code runs.
- One `getRandomValues()` request is limited to the Web Crypto 65,536-byte
  maximum; integer typed-array validation, UUID v4 formatting, and explicit
  error behavior remain unchanged.
- Repeated random requests work in both page and worker realms without a
  deterministic or `Math.random()` fallback.

## Path

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- `docs/architecture/native-engine.md`

## Implementation

The Rust runtime installs a short-lived QuickJS function backed directly by
`getrandom::fill` before each document, module, or worker bootstrap. The page
and worker crypto closures capture it, compact consumed pool bytes, request
only missing capacity, validate the returned array, and then continue through
the existing bounded pool. The public helper is deleted from the global
object after bootstrap installation.

## Tradeoffs and follow-up

The synchronous host call removes artificial pool exhaustion but performs an
OS request when the fast path is empty. The request remains bounded to avoid
large allocations and denial-of-service amplification. Web Crypto `subtle`,
`CryptoKey`, digest/sign/encrypt operations, and complete Web IDL descriptors
remain separate issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --features native-engine --tests --locked`
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine crypto --locked -- --nocapture`
- `git diff --check`

The evidence is local-only. Remote CI, registry publication, release, and
final native/CDP parity or production-promotion claims remain pending the
wider issue #40 gates.
