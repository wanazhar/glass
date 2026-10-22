# Shared Cargo workspace for Glass and Glass Core

## Decision

Glass Core's Rust packages become members of the repository's root Cargo
workspace while retaining their product identity and standalone entry points.
The shared root workspace version is the single Rust package version for both
products. Its initial value is Glass's current source version, `0.3.14`.
Cargo metadata, installer output, product documentation, and release checks
must agree with that value. Historical Core tags and commit objects remain
unchanged; synchronization applies to new source commits and future releases.

The `glass-core` package and binary remain independently installable with
`cargo build -p glass-core` and `cargo install --path crates/glass-core` (or
the product installer). They must not pull in `glass-dev`, `glass-browser`,
`glass-browser-core`, or the native browser worker. `gc` remains the installer
alias. The root workspace's default members remain Glass-only so ordinary
Glass build loops do not compile the Core product.

## Package and asset layout

Move Rust packages from `products/glass-core/crates/` into the root `crates/`
directory: `glass` becomes `crates/glass-core`, while `glass-mod-api`,
`glass-runtime-client`, and `glass-tui` retain their package names. Keep
product-owned non-Rust files under `products/glass-core/` (web UI, extensions,
installer, fixtures, scripts, npm workspaces, licenses, and product docs).
Update compile-time asset paths and runtime path discovery explicitly; do not
duplicate embedded assets or rely on the process working directory.

The repository has one Cargo lockfile. Preserve Core's product-specific
license metadata and license files, because the Core workspace declares MIT OR
Apache-2.0 while its checked-in license files and notices have their own
scope. Do not inherit Glass's root MIT license for Core packages. Resolve
release-profile differences deliberately and record the resulting build
profile behavior; do not imply old performance characteristics without
measurement.

## Dependency and integration boundaries

Core remains a standalone product within the shared workspace. A dependency
direction check must prove that building/installing `glass-core` does not
select Glass browser or development packages. Existing runtime protocol and
product behavior are unchanged by this structural slice. Subsequent #63 work
will define the negotiated mixed Core-coding/Glass-integration tool contract
as a separate design and implementation slice.

## Verification

Validate workspace metadata and dependency closure; build the Core binary with
an isolated or existing safe target directory; run Core's focused previously
failing Rust tests and focused package tests; run the product's installer
smoke test with an isolated install root and verify `glass-core` plus `gc`;
verify web and extension asset paths; run Glass's package-scoped checks and
documentation/version validators. Compare the moved files to the imported
source commit after accounting only for intentional manifest, path, and
version changes. Confirm every imported source commit remains reachable and
that the old imported root lockfile/workspace no longer controls Cargo.
