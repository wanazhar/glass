# Standalone Glass Core

Glass Core is the coding harness used by Glass resident agents. It is also a
standalone product: its `glass-core` package, `gc` command alias, terminal
interface, optional web assets, Cargo workspace, lockfile, and license notices
remain usable without installing Glass Dev or its browser packages.

The source is under [`products/glass-core`](../products/glass-core/README.md).
Its workspace package version is declared independently in its own
[`Cargo.toml`](../products/glass-core/Cargo.toml), and its license declaration
is `MIT OR Apache-2.0`. The source tree retains its MIT license files and
[`THIRD_PARTY.md`](../products/glass-core/THIRD_PARTY.md). The root Glass Cargo
workspace and its version are maintained separately.

## Build from the monorepo

Run these commands from the repository root. `--manifest-path` selects the
nested workspace, so Cargo uses Glass Core's own lockfile and package metadata.

```sh
cargo metadata --manifest-path products/glass-core/Cargo.toml --locked --format-version 1
cargo build --manifest-path products/glass-core/Cargo.toml --locked --package glass-core --bin glass-core
```

The manifest contains four packages: `glass-core`, `glass-tui`,
`glass-mod-api`, and `glass-runtime-client`. This build does not select the
Glass Dev or browser packages from the root workspace.

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
changes under `products/glass-core/`. GitHub does not discover a workflow stored
inside that nested product directory, so the root workflow sets its working
directory there and validates the independent workspace, its packages, web
assets, and isolated installer.
