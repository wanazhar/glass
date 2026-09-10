---
id: native-engine-browser-124
scope: glass-browser/native-engine/url-authority-setters
status: done
depends-on: [native-engine-browser-123]
---

# BE-38: bounded URL authority and protocol setters

## Objective

Extend the live native URL owner with the common HTTP(S) authority and protocol
mutations needed by request-building and redirect-like application code.

## Contract

- `protocol` accepts bounded `http:`/`https:` values and refreshes the URL's
  origin and serialized href in place.
- `host`, `hostname`, and `port` update the same authority while retaining the
  path, query, fragment, credentials, and `searchParams` owner.
- `username` and `password` use bounded URI-component encoding and retain live
  authority state; getters expose the serialized credential components.
- Invalid hosts, unsupported protocols, non-numeric/out-of-range ports, and
  authority mutations without a host fail explicitly.
- Existing URL, Request, Fetch, query synchronization, and relative-resolution
  behavior remains unchanged.

## Ownership and sequence

```text
URL owner -> authority/protocol setter -> same component state
                                   -> same searchParams owner
                                   -> refreshed origin/href
```

Setters rebuild the current bounded HTTP(S) URL through the existing parser and
replace component state in place. The outer URL object remains frozen while its
private owner closures stay mutable.

## Deliberate boundary and tradeoffs

This slice covers common HTTP(S) host and credential rewrites without claiming
the full WHATWG URL algorithm. Full percent-encoding rules, IDNA, IPv6 edge
cases, default-port canonicalization, non-HTTP scheme mutation, descriptors,
and complete URL/Web IDL parity remain open. Failing closed for unsupported
authority input avoids sending an ambiguous URL into the Rust transport policy.

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
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_exposes_bounded_script_fetch_promises -- --nocapture` — 1 passed, including positive and fail-closed authority setter assertions
- `cargo fmt --all -- --check` and `git diff --check` — passed
- `python3 scripts/check-documentation-coverage.py` — 774 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  774 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=948; current-claim failures=0
