# Native engine browser-complete slice 478: rooted file workers

- Status: complete
- Scope: `native-engine` / dedicated and shared rooted-file workers
- Issue: #40
- Depends on: [native-engine-browser-477](native-engine-browser-477.md)

## Objective

Allow a rooted file document to create ordinary dedicated and shared workers
through the existing native worker owner, including bounded classic
`importScripts()` graphs and module dependency graphs.

## Contract

- A file document may create a classic or module `Worker` or `SharedWorker`
  whose resolved URL is a credential-free rooted `file:` resource.
- Worker source and dependencies use the existing bounded file loader, root,
  symlink, UTF-8, size, and graph limits; no network/cache fallback is added.
- Relative classic `importScripts()` and module imports resolve against the
  owning worker file URL. Existing fixture and HTTP(S) worker behavior remains
  unchanged.
- File workers preserve the existing isolated worker realm, message/error,
  timer, transfer, and shared-name ownership contracts.

## Implementation

- Admit rooted file worker sources in the resource loader.
- Permit the worker and shared-worker JS constructors to pass rooted file
  URLs, while keeping non-file origin and credential checks intact.
- Extend worker module URL admission to file URLs and cover classic/shared
  workers plus nested dependency loading in file-backed integration tests.

## Tradeoffs and remaining scope

This slice reuses the current worker runtime and does not claim service-worker
registration for file origins, worklets, import maps, complete worker Web IDL
parity, or broader file-origin semantics. Those remain issue #40 gates.

## Verification

- `cargo fmt --all`
- `cargo check --quiet -p glass-browser --test native_engine --locked`
- `cargo test --quiet -p glass-browser --test native_engine --locked native_file_` (8 passed)
- `python3 scripts/check-release-documentation.py --require-previous-version --report /tmp/glass-release-documentation-478.json`
- `python3 scripts/check-documentation-depth.py`
- `python3 scripts/check-tui-shortcuts.py`
- `git diff --check`
