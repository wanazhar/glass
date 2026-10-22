# Glass Core monorepo migration

Status: Historical migration evidence. This record describes source and
publication state observed on 2026-10-22. It does not close umbrella issue
[#63](https://github.com/wanazhar/glass/issues/63) or claim completion of its
runtime ownership and TUI workstreams.

## Source refs and history

The Glass checkout was clean on local `main` at
`d2aa0b0e0c27f9f62f0666c0a587b58bf79e1e0d`, three commits ahead of its freshly
fetched `origin/main` at `eeb17a9accdd8a4aa93e562197cfa5a133114e86`. The import
keeps those Glass commits as its first-parent history.

The Glass Core checkout was clean on its tracked files. Freshly fetched
`origin/main` and local `main` both pointed to
`5ae9fdfce07dff6104e096391511ea532fc94a17`. The advertised feature heads were
`fcf804331c2ae6c744246cc1e47ee72e70255598` and
`4e95ea69eb9282048b36233ab8a5d464a32560fb`; both are already ancestors of Core
`main`. Core had no tags. Its local `master` pointed to
`3098046e2abccddf419d9efc9fd6c2c21b802c31`, one commit outside `main`.

The dedicated import commit is
`0fbbe338457abf8197ae1f486be94509cdebf4ec`. Its parents, in order, are:

1. Glass `main` at `d2aa0b0e0c27f9f62f0666c0a587b58bf79e1e0d`;
2. Core `main` at `5ae9fdfce07dff6104e096391511ea532fc94a17`;
3. Core `master` at `3098046e2abccddf419d9efc9fd6c2c21b802c31`.

Core `master` has the same tree as Core `main`; its one unique commit changes
no files. The third parent retains that commit and all of its ancestry without
applying master-only content. The migration commit adds the tracked Core tree
under `products/glass-core/`. At that commit, the subtree tree ID was
`5d62c433947e238dfa4465f1dceabd8e0f0420a1`, equal to Core `main`'s tree, with
547 tracked paths.

The source commit refs are retained in the local `refs/glass-core/original/`
namespace. The public archive branches mirror the original source heads:

| Core source head | Public Glass archive branch | Tip |
|---|---|---|
| `main` | `archive/glass-core/main` | `5ae9fdfce07dff6104e096391511ea532fc94a17` |
| `master` | `archive/glass-core/master` | `3098046e2abccddf419d9efc9fd6c2c21b802c31` |
| `feat/web-collaboration-and-image-sharing` | `archive/glass-core/feat/web-collaboration-and-image-sharing` | `fcf804331c2ae6c744246cc1e47ee72e70255598` |
| `fix/runtime-workspace-tool-catalog` | `archive/glass-core/fix/runtime-workspace-tool-catalog` | `4e95ea69eb9282048b36233ab8a5d464a32560fb` |

The fetched `origin/*` refs pointed to those same four commits; Core had no
source tags. The source remote-tracking refs are also represented under
`archive/glass-core/remotes/origin/`, with the same commit IDs as the local
heads. The original Core checkout and its local state remain in place.
Ephemeral task-snapshot refs in that checkout pointed to tree objects rather
than commits and are not public archive branches.

## Public-source and license audit

The audit covered the commit history reachable from Core's local and advertised
heads. The checksum-verified Gitleaks 8.30.1 scanner scanned 270 commits and
reported six JWT-pattern matches in
`crates/glass/src/testdata/chatgpt-id-tokens.json` at commit
`378073650720038d46fd6096cb911b910a298dce`. The file labels these as public
test fixtures; its note states that the throwaway private key was discarded.
Only the public verification keys and signed test strings are present. The
matches cover valid, expired, issuer, future, missing-subject, and
multiple-audience test cases. No credentials or private keys were found.

An additional pattern scan found only explicit redaction sentinels and the
standard AWS example key in tests. It found no tracked environment files,
private-key files, or generated dependency/build directories in the Core main
tree. Ignored state such as `.glass`, `.freebuff`, `node_modules`, and `target`
was not imported.

Core retains its `MIT OR Apache-2.0` workspace declaration, root MIT license,
package MIT license files, and `THIRD_PARTY.md`. Glass's root MIT license and
Cargo workspace remain separate. The four Core Cargo packages now publish the
monorepo repository URL through their independent workspace metadata.

## Build and publication boundary

The root Glass Cargo workspace was not changed. Core keeps its own manifest,
lockfile, profiles, version, scripts, documentation, and assets at
`products/glass-core/`. Root workflows explicitly run Core checks from that
nested workspace because GitHub does not discover nested workflow files.
The import commit preserves the exact source tree; a focused follow-up commit
adds root workflow and documentation integration, Cargo repository metadata,
and two semantics-preserving `collapsible_if` cleanups. The latter were needed
to keep the source CI's existing `clippy -D warnings` gate green; both warnings
were reproduced against a clean archive of the source `main` commit first.

Bounded validation on the nested workspace passed Cargo metadata, optimized
binary build, package generation, the install script with an isolated install
root, npm tests, and web asset build. The smoke install verified both
`glass-core` and its `gc` symlink report version 0.1.0 and that the bundled
web index is present. The nested Rust test suite has five existing failures;
the same 878 passed / 5 failed result reproduced from a clean source-main
archive. The two source Clippy findings were fixed in the follow-up. The
imported source also does not pass `cargo fmt --all -- --check`; the immutable
source snapshot was not broadly reformatted, and this is not a CI gate.

The migration commit and follow-up integration commit are intended for Glass
`main`, with source head copies under `archive/glass-core/`; publication is
recorded here after remote verification. The old Core repository remains
unchanged and is not archived by this migration. Archive it only after the
standalone install and user path transition have been verified.

The migration is one milestone in issue #63. Runtime ownership, shared tool
execution, and the TUI work remain open acceptance work.
