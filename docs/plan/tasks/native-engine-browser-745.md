---
id: native-engine-browser-745
scope: glass-browser/content-worker-address-space-ceiling
status: in-progress
depends_on: [native-engine-browser-744]
---

# Glass native-engine browser slice 745: content-worker memory ceiling

## Objective

Put an OS-enforced upper bound on address-space/committed-memory growth for
the native content worker. Browser-level object and document limits do not
bound all allocations made by the runtime, parser, style/layout code, network
stack, and JavaScript VM.

## Contract

- Set a 1 GiB per-worker ceiling on Linux using `RLIMIT_AS`, applied before
  Bubblewrap exec so the limit is inherited by the content worker. Preserve a
  stricter inherited soft or hard limit rather than raising it.
- Set a 1 GiB per-process committed-memory ceiling in the Windows Job Object,
  alongside its existing one-process and kill-on-job-close limits.
- Failure to apply the Windows job limit is a sandbox-start error. Linux
  pre-exec failure must prevent the Bubblewrap worker from starting; there is
  no unsandboxed retry.
- Describe these as platform-specific address-space/commit guards, not as an
  equivalent RSS cap, complete resource quota policy, or measured performance
  budget. macOS limits and all-platform tuning remain open.
- On Linux, verify the inherited limit in a subprocess and rerun a
  process-backed page test under the capped Bubblewrap path. Cross-check the
  Windows target so the Job Object configuration is type-correct.

## Tradeoffs

A page requiring more than the configured ceiling can fail allocation and its
content worker may terminate; the owning Glass session must surface the typed
worker failure and remain recoverable. The 1 GiB initial ceiling is a safety
guard, not the final corpus-derived value. Promotion still requires workload
measurements and platform-specific budgets. Windows Job Object memory limits
bound committed memory, while Linux `RLIMIT_AS` bounds virtual address space;
these are not identical metrics.

## Paths

- `crates/glass-browser/src/browser/native_engine/sandbox.rs`
- `docs/plan/native-engine-browser-profile.md`
- `docs/architecture/native-engine.md`
- `docs/plan/README.md`
- `docs/plan/tasks/native-engine-browser-745.md`

## Verification

- Run `cargo check -p glass-browser --lib --tests --locked --quiet` before
  focused tests.
- Cross-check `glass-browser` for `x86_64-pc-windows-msvc` without installing
  additional toolchains or building/linking release binaries.
- Run the Linux address-space-limit unit test and the focused
  process-backed page-loading regression.
- Run formatting, `git diff --check`, and repository documentation truth,
  depth, shortcut, and coverage checks. Record which checks are Linux-local;
  no Windows runtime or macOS behavior may be claimed from cross-compilation.

## Evidence so far

- Linux `cargo check -p glass-browser --lib --tests --locked --quiet` passed.
- The Linux namespace and address-space sandbox tests passed (2 tests); the
  existing process-backed layout/navigation regression passed (1 test) under
  the new ceiling.
- The Windows `cargo check` was attempted for the installed
  `x86_64-pc-windows-msvc` target but stopped in the `ring` dependency before
  compiling `glass-browser`: this host has no MSVC-compatible C compiler. The
  Windows Job Object change was independently type-checked against the
  already-built `windows-sys` metadata; full-crate cross-compilation and
  Windows runtime behavior remain unverified pending a Windows runner.
- Documentation and final formatting gates are recorded after closeout; slice
  status remains in progress until the Windows implementation is validated.
