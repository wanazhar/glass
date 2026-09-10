---
id: native-engine-browser-123
scope: glass-browser/native-engine/url-component-setters
status: done
depends-on: [native-engine-browser-122]
---

# BE-37: bounded URL component setters

## Objective

Extend the live URL owner with bounded `pathname` normalization and `href`
replacement while preserving query/fragment/search-parameter identity.

## Contract

- `url.pathname = value` applies bounded slash/dot/dot-dot normalization and
  retains the URL's current query and fragment state.
- `url.href = value` resolves the replacement against the current URL when
  relative, refreshes the bounded URL components, and updates the existing
  `searchParams` object rather than replacing it.
- Existing live `searchParams`, `search`, and `hash` synchronization remains
  active after either setter; Request/Fetch URL-object handoff observes the
  current href.
- Authority, protocol, username/password, host/hostname/port setters,
  complete URL parser/encoding/IDNA/IPv6/default-port behavior, and complete
  URL/Web IDL identity remain open.

## Ownership and sequence

```text
URL owner -> pathname/href setter -> same component state
                                   -> same searchParams owner
```

`href` replacement refreshes component state in place. The URL object stays
frozen at the outer property level while its bounded setter closures mutate the
private owner state.

## Deliberate boundary and tradeoffs

This supports common URL rewrites without implementing authority/protocol
mutation or a second parser. Path normalization is deliberately bounded and
delegates transport/origin enforcement to the existing Rust/content-worker
boundary.

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
- `python3 scripts/check-documentation-coverage.py` — 773 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  773 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=947; current-claim failures=0
