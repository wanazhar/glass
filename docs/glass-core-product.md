# Standalone Glass Core

Glass Core is the coding harness used by Glass resident agents. It is also a
standalone product: its `glass-core` package, `gc` command alias, terminal
interface, optional web assets, installer, and license notices remain usable
without installing Glass Dev or its browser packages. Glass negotiates runtime
protocol v2 with the `mixedTools` capability; the standalone runtime keeps its
local and generic host modes.

The Rust packages live under [`crates/`](../crates/glass-core/Cargo.toml) in
the root Cargo workspace. They share the root package version, currently
`0.3.14`, while retaining `MIT OR Apache-2.0` package metadata and the
product's own license files and [`THIRD_PARTY.md`](../products/glass-core/THIRD_PARTY.md).
Web assets, TypeScript extensions, scripts, and product documentation remain
under [`products/glass-core`](../products/glass-core/README.md). The Core
packages are explicit workspace members but are not default members, so normal
Glass builds stay Glass-only.

## Build from the monorepo

Run these commands from the repository root. Cargo uses the shared root lockfile
and the Core release profile preserves its size-oriented optimization and fat
LTO settings.

```sh
cargo metadata --locked --format-version 1
cargo tree -p glass-core --locked
cargo build --profile core-release --locked -p glass-core --bin glass-core
```

The Core package set is `glass-core`, `glass-tui`, `glass-mod-api`, and
`glass-runtime-client`. Building or installing `glass-core` selects only this
package closure; it does not pull in Glass Dev or browser packages.

## Install the current checkout

The Core installer builds and installs only `glass-core`, then creates the
relative `gc` symlink beside it:

```sh
products/glass-core/scripts/install.sh
glass-core --version
gc --version
```

For a disposable install, set `CARGO_INSTALL_ROOT` to a temporary directory.
The optional web bundle installs separately under the Cargo root's
`share/glass-core/web` directory:

```sh
CARGO_INSTALL_ROOT=/tmp/glass-core-install \
GC_INSTALL_WEB=1 \
  products/glass-core/scripts/install.sh
```

The installer checks that `gc` is absent or already points to `glass-core`
before replacing it. See [Glass Core resident harness](glass-core-harness.md)
for how Glass discovers the executable and reports provider readiness.

## CI and validation scope

The root [Glass Core workflow](../.github/workflows/glass-core-ci.yml) runs for
changes to the Core crates or product assets. The root workflow selects the
Core package set while using the root lockfile and the product working
directory for npm and smoke scripts.
