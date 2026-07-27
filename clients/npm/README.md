# `@glass-browser/cli`

This package downloads the matching stripped Glass binary from a GitHub
release during `npm install`, verifies its SHA-256 checksum, and exposes it as
the `glass` command on Linux x64 and macOS x64/arm64. Set
`GLASS_VERSION=vX.Y.Z` to select a release or `GLASS_SKIP_DOWNLOAD=1` when
supplying a local binary yourself.

The package version tracks the native release it downloads. To select another
release explicitly, set `GLASS_VERSION=vX.Y.Z` before installing.
