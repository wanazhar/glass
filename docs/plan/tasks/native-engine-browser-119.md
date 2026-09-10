---
id: native-engine-browser-119
scope: glass-browser/native-engine/url-objects
status: done
depends-on: [native-engine-browser-118]
---

# BE-33: bounded URL objects and URL Fetch inputs

## Objective

Expose a bounded native `URL` object for common HTTP(S) relative resolution,
component inspection, and Fetch/Request input handoff.

## Contract

- `new URL(input, base)` resolves absolute HTTP(S), protocol-relative, path,
  query, and fragment references against an HTTP(S) base with bounded dot/
  dot-dot path normalization.
- URL objects expose bounded `href`, `origin`, `protocol`, `username`,
  `password`, `host`, `hostname`, `port`, `pathname`, `search`, `hash`, and a
  snapshot `searchParams` view, plus `toString()`/`toJSON()`.
- `new Request(urlObject, init)` and `fetch(urlObject, overrides)` use the
  normalized URL href while retaining the existing Request/Fetch validation,
  CORS, redirect, abort, and transport owners.
- Existing string URL and `URLSearchParams` behavior remains unchanged.
- URL mutation/setter synchronization, full percent-encoding/IDNA/IPv6 and
  default-port normalization, non-HTTP scheme parity, live `searchParams`
  synchronization, URLPattern, complete URL/Web IDL identity, and browser-wide
  URL parser parity remain open.

## Ownership and sequence

```text
URL input + base -> bounded URL projection -> Request/Fetch href
                                      \-> snapshot URLSearchParams owner
```

The URL projection is a bounded JavaScript owner. Rust/content-worker URL and
origin policy remains authoritative once the normalized href enters Fetch or
navigation.

## Deliberate boundary and tradeoffs

This covers common application URL construction without duplicating the full
WHATWG URL parser in the content worker. The immutable component snapshot and
HTTP(S)-focused normalization are explicit; mutable URL setters and complete
Unicode/encoding parity are not implied.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

The slice was validated with the affected native-engine integration target.
The issue-level full native-engine suite, strict-Clippy baseline,
documentation, release-truth, remote-CI, publication, and browser-parity
gates remain final issue gates; this task makes no remote or release claim.

- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed
- `cargo fmt --all -- --check` and `git diff --check` — passed
- `python3 scripts/check-documentation-coverage.py` — 769 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  769 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=943; current-claim failures=0
