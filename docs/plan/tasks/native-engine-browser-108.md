---
id: native-engine-browser-108
scope: glass-browser/native-engine/urlsearchparams-iterables
status: done
depends-on: [native-engine-browser-107]
---

# BE-29: bounded URLSearchParams iterable inputs

## Objective

Accept bounded iterable pair inputs in the existing URLSearchParams
constructor without adding another query or request-body owner.

## Contract

- Map, Set, and other inputs exposing `Symbol.iterator` are consumed as
  ordered pair sequences. Each pair must itself be iterable and contain exactly
  two values; malformed pairs fail closed with `TypeError`.
- Names and values pass through the existing string conversion, entry-count,
  key-length, and value-length bounds. Insertion order remains observable
  before `sort()` is requested.
- Existing string, array-pair, record, and native URLSearchParams inputs retain
  their established behavior, as do mutation, encoding, body serialization,
  and snapshot iterators.
- Live iterator mutation/identity, arbitrary Web IDL exotic conversions, and
  complete URLSearchParams parity remain open.

## Ownership and sequence

```text
iterable input -> bounded pair validation -> _entries -> existing URL encoding
```

The JavaScript constructor owns iterable validation. The existing `_entries`
list remains the sole source for URL encoding, sorting, iteration, and Fetch/XHR
body transfer.

## Deliberate boundary and tradeoffs

The implementation supports ordinary pair iterables used by modern code while
retaining a small deterministic pair contract. It does not expose a general
iterator adapter or claim live mutation semantics, reducing surface area and
resource risk at the cost of full Web IDL parity.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

The slice was validated with the affected native-engine target and the
URLSearchParams integration test. The issue-level full native-engine suite,
strict-Clippy baseline, documentation, release-truth, remote-CI,
publication, and browser-parity gates remain final issue gates; this task
makes no remote or release claim.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_fetches_bounded_url_search_params -- --nocapture` — 1 passed
- `python3 scripts/check-documentation-coverage.py` — 758 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  758 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=932; current-claim failures=0
