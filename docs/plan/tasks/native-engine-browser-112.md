---
id: native-engine-browser-112
scope: glass-browser/native-engine/urlsearchparams-live-iterators
status: done
depends-on: [native-engine-browser-111]
---

# BE-29: bounded live URLSearchParams iterators

## Objective

Make the existing URLSearchParams iterator methods live and self-iterating
without adding a second entry store or changing request-body serialization.

## Contract

- `entries()`, `keys()`, and `values()` retain the owning URLSearchParams
  instance and observe bounded later mutations through the existing `_entries`
  list rather than an eager copy.
- Each iterator exposes `next()` and returns itself from `[Symbol.iterator]()`;
  `URLSearchParams.prototype[Symbol.iterator]` remains the same method as
  `entries`.
- Appends and value updates made after iterator creation are observable at the
  current cursor. Existing string/record/pair/iterable construction, sorting,
  `forEach()` snapshot behavior, encoding, and Fetch/XHR body transfer remain
  stable.
- Entry counts and values remain subject to the existing bounds. Complete
  Web IDL prototype/descriptor parity and every browser-specific mutation
  corner case remain open.

## Ownership and sequence

```text
URLSearchParams owner -> live cursor -> current bounded _entries list
  -> existing encoding/fetch body owner
```

The JavaScript URLSearchParams object remains the only entry owner. Iterators
hold only the owner reference, kind, and cursor index.

## Deliberate boundary and tradeoffs

The slice makes common iterator identity and append/update visibility usable
without allocating an unbounded snapshot per iterator. It intentionally keeps
the existing compact cursor behavior for complex deletion/reordering races and
does not claim complete Web IDL mutation semantics.

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

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_url_search_params_iterators_are_live_and_self_iterating -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_fetches_bounded_url_search_params -- --nocapture` — 1 passed
- `python3 scripts/check-documentation-coverage.py` — 762 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  762 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=936; current-claim failures=0
