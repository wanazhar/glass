# Native engine browser-complete slice 453: URLSearchParams value filters

- Status: complete locally
- Scope: `native-engine` / URLSearchParams page-worker parity
- Issue: #40
- Depends on: [native-engine-browser-452](native-engine-browser-452.md)

## Objective

Make the page and dedicated/SharedWorker `URLSearchParams` projections honor
the optional value argument of `has(name, value)`. A query-name match alone
must not report success when the caller supplies a different value.

## Contract

- `has(name)` returns whether any entry has the string-converted name.
- `has(name, value)` returns whether an entry has both the string-converted
  name and string-converted value.
- Page and worker realms expose the same overload behavior, without changing
  insertion order, duplicate handling, live URL synchronization, or the
  existing bounded entry/name/value limits.

## Implementation

- Added value-sensitive matching to the page `URLSearchParamsNative.has`
  implementation. The worker projection already had the same bounded
  overload behavior and now has a matching cross-realm witness.
- Extended the local page and worker URL runtime fixtures with positive and
  negative value-filter assertions.

## Tradeoffs

The optional value comparison is implemented at the realm-owned collection
boundary, so it adds no host call or transport cost. Page and worker bootstrap
implementations remain duplicated; a later shared Web IDL layer can reduce
drift while preserving separate realm ownership. Complete URLSearchParams
encoding, descriptor, and Web IDL parity remain outside this slice.

## Evidence

- `cargo fmt --all`
- `cargo check -p glass-browser --test native_engine --locked`
- `cargo test -p glass-browser --test native_engine --locked native_local_worker_exposes_url_search_params_and_navigator -- --nocapture` (1 passed)
- `cargo test -p glass-browser --test native_engine --locked native_local_url_search_params_iterators_are_live_and_self_iterating -- --nocapture` (1 passed)
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-453.json`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `python3 scripts/check-documentation-coverage.py`
- `git diff --check`
