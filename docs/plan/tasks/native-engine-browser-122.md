---
id: native-engine-browser-122
scope: glass-browser/native-engine/url-searchparams-live-sync
status: done
depends-on: [native-engine-browser-121]
---

# BE-36: bounded live URL search-parameter synchronization

## Objective

Make each bounded native URL's `searchParams` owner synchronize its serialized
query back to URL `search` and `href` state.

## Contract

- `url.searchParams.append()`, `set()`, `delete()`, and `sort()` update the
  owning URL's bounded `search` and `href` values while preserving the same
  `searchParams` object identity.
- URL `search` assignment replaces the search-parameter entries and updates
  the serialized query; `hash` assignment updates the URL fragment while
  retaining query state.
- Request and Fetch URL-object handoff observes the current URL `href` after a
  bounded search-parameter mutation.
- Existing URL component inspection, relative resolution, URLSearchParams
  encoding, and string URL behavior remain unchanged.
- Full URL setter/parser parity, live encoding/Unicode/percent-decoding
  semantics, default ports/IDNA/IPv6, URLSearchParams descriptor identity, and
  browser-wide URL/Web IDL parity remain open.

## Ownership and sequence

```text
URL owner -> same URLSearchParams owner -> serialized search -> href getter
```

The URL owns the synchronization closure; it does not replace the
`searchParams` object when query entries mutate. Rust/content-worker URL and
origin policy remains authoritative after Fetch/navigation handoff.

## Deliberate boundary and tradeoffs

Live query synchronization enables common URL-building flows without claiming
the full mutable WHATWG URL algorithm. Only the bounded query and fragment
mutators are wired; other component setters and complete encoding behavior
remain explicit future gates.

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
- `python3 scripts/check-documentation-coverage.py` — 772 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  772 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=946; current-claim failures=0
