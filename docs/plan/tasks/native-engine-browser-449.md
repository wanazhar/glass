# Native engine browser-complete slice 449: worker URL mutability

status: complete
scope: native-engine/worker-url-mutation
issue: 40
depends-on: [native-engine-browser-448]

## Objective

Close the dedicated/SharedWorker `URL` object mutability gap without changing
the worker `location` security boundary. Worker-created URL objects must expose
the same bounded mutation and live query-parameter behavior as page-created
URL objects.

## Normative reference

The object and URL state model follows the [WHATWG URL
Standard](https://url.spec.whatwg.org/). The worker global and location
separation remains governed by the worker-facing [HTML Standard](https://html.spec.whatwg.org/)
model; Glass retains its explicit realm and resource limits.

## Contract

- Worker-created `URL` instances expose mutable `href`, `protocol`,
  `username`, `password`, `host`, `hostname`, `port`, `pathname`, `search`,
  and `hash` accessors, with canonical URL state reflected after each accepted
  mutation.
- Worker `URL.searchParams` is a live object. Its bounded `append`, `set`,
  `delete`, and `sort` operations update the owning URL's serialized query;
  replacing `search` rebuilds the parameter list and preserves the same live
  object identity.
- Authority, port, credential, protocol, and path mutations retain the
  existing bounded validation and fail with `TypeError` without corrupting the
  previous URL. Worker `location` remains a frozen read-only projection.
- The native Rust URL source remains the canonical parser for accepted initial
  values, relative resolution, path normalization, and fragment escaping;
  this slice does not add a second transport or policy owner.

## Implementation

- Replaced the read-only worker URL projection with a private state record and
  accessor-backed mutable URL object, matching the existing page-side bounded
  authority and credential mutation contract.
- Added live query synchronization around the worker URL-search-parameter
  mutators and rebuilt the parameter owner whenever `href` or `search` is
  replaced.
- Routed worker pathname and hash setter normalization through the already
  installed bounded Rust canonicalizer, including explicit undefined-base
  calls for absolute setter normalization.
- Added a worker integration witness covering `instanceof URL`, path and query
  mutation, fragment escaping, live parameter reads, and rejected protocol
  mutation.

## Tradeoffs

- The worker object now shares the page object's useful mutation contract, but
  the two realm projections remain duplicated bootstrap code; a future shared
  Web IDL descriptor layer can reduce drift once the full surface is defined.
- Worker URL mutation is synchronous and bounded, so it preserves deterministic
  script turns at the cost of a small canonicalizer call for path and fragment
  setter updates.
- Complete URL descriptor identity, all setter edge cases, blob/file origins,
  and the full browser scheme matrix remain issue #40 work.

## Evidence

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine native_local_worker_url_objects_are_mutable_and_search_params_are_live --locked -- --nocapture` (1 passed)
- `cargo test --quiet -p glass-browser --test native_engine url --locked -- --test-threads=1 --nocapture` (8 passed, 0 failed)
- `git diff --check`
