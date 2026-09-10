---
id: native-engine-browser-114
scope: glass-browser/native-engine/formdata-live-iterators
status: done
depends-on: [native-engine-browser-113]
---

# BE-29: bounded live FormData iterators

## Objective

Make the existing FormData iterator methods live and self-iterating without
creating a second entry store or changing multipart serialization.

## Contract

- `entries()`, `keys()`, and `values()` retain the owning FormData instance and
  observe bounded later mutations through the existing `_entries` list.
- Each iterator exposes `next()` and returns itself from `[Symbol.iterator]()`;
  `FormData.prototype[Symbol.iterator]` remains the same method as `entries`.
- Appends and value updates made after iterator creation are visible at the
  current cursor. Existing text/File/Blob value normalization, multipart
  serialization, `forEach()` traversal, and entry bounds remain stable.
- Complex deletion/reordering mutation semantics and complete FormData/Web IDL
  prototype/descriptor parity remain open.

## Ownership and sequence

```text
FormData owner -> live cursor -> current bounded _entries list
  -> existing multipart serializer
```

The JavaScript FormData object remains the only entry owner. Iterators hold only
the owner reference, kind, and cursor index.

## Deliberate boundary and tradeoffs

Common iterator identity and append/update visibility are now available without
allocating an eager snapshot per iterator. The compact cursor behavior for
complex delete/reorder races is retained and does not claim full Web IDL
mutation semantics.

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
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_local_form_data_iterators_are_live_and_self_iterating -- --nocapture` — 1 passed
- `cargo test --quiet -p glass-browser --features native-engine --test native_engine native_content_process_fetches_bounded_text_form_data -- --nocapture` — 1 passed
- `python3 scripts/check-documentation-coverage.py` — 764 Markdown files,
  345 full-product MCP tools (100 browser-only), 17 examples, 22 public
  modules
- `python3 scripts/check-documentation-depth.py` — 93 current guides,
  19 substantive contracts
- `python3 scripts/check-release-documentation.py --require-previous-version` —
  764 Markdown documents; current documents=83; previous-version hits=59;
  semantic audit hits=938; current-claim failures=0
