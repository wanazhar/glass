---
id: native-engine-browser-107
scope: glass-browser/native-engine/formdata-iterators
status: done
depends-on: [native-engine-browser-106]
---

# BE-28: bounded FormData iterators

## Objective

Expose deterministic iterator objects for the existing bounded FormData entry
list without adding another serialization or transport owner.

## Contract

- `entries()`, `keys()`, and `values()` return fresh bounded snapshot
  iterators. Each iterator exposes `next()` records with deterministic
  completion and returns itself from `[Symbol.iterator]()`.
- FormData `[Symbol.iterator]()` aliases `entries()`, so `for...of` and
  `Array.from()` observe ordered name/value pairs. File entries yield their
  existing native Blob/File object values; text entries yield strings.
- `forEach()` and multipart Fetch/XHR serialization retain the existing
  insertion order, duplicate-name behavior, bounds, and value normalization.
- Iterators intentionally snapshot the bounded entry list. Live mutation during
  iteration, exotic iterable constructors, and complete FormData/Web IDL
  parity remain open.

## Ownership and sequence

```text
FormData._entries -> snapshot mapping -> self-iterating next() object
       \-> forEach/multipart serializer retain the same source list
```

The JavaScript host wrapper owns iterator identity and value projection. The
existing bounded `_entries` list remains the sole source for form submission
and network multipart encoding.

## Deliberate boundary and tradeoffs

Snapshot iterators make the public shape compatible with ordinary iteration
without exposing live mutation semantics or introducing a second data model.
This keeps memory proportional to the bounded FormData entry budget but does
not claim the full platform's live/exotic iterator behavior.

## Paths

- `crates/glass-browser/src/browser/native_engine/javascript.rs`
- `crates/glass-browser/tests/native_engine.rs`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/analysis/native-engine.md`
- GitHub issue #40

## Verification

The slice was validated with the affected native-engine target and all
FormData-focused integration coverage. The issue-level full native-engine
suite, strict-Clippy baseline, documentation, release-truth, remote-CI,
publication, and browser-parity gates remain final issue gates; this task
makes no remote or release claim.

- `cargo fmt --all -- --check` — passed
- `git diff --check` — passed
- `cargo check --quiet -p glass-browser --features native-engine --test native_engine` — passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine form_data -- --nocapture` — 5 passed, 0 failed
- `python3 scripts/check-documentation-coverage.py` — 757 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  757 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=931; current-claim failures=0
