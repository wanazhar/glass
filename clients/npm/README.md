# @glass-browser/cli

This package downloads the matching stripped Glass binary during `npm install`.
It verifies the SHA-256 checksum. It installs the `glass` command for:

- Linux x64;
- Linux arm64;
- macOS x64; and
- macOS arm64.

Windows is not a supported target.

Set `GLASS_VERSION=vX.Y.Z` to select a release. Set
`GLASS_SKIP_DOWNLOAD=1` when you provide a local binary.

The package version must match the native Glass release version.
