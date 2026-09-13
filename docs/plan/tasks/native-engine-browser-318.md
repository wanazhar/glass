# Glass native engine browser slice 318: profile and viewport ownership

Status: completed locally; issue #40 browser-profile conformance, persistent
workflow ownership, and remote CI remain open.

## Objective

Make native CLI and TUI startup use the same product configuration boundary
for viewport and profile state. A native session must not silently inherit
Chromium's profile manager or discard the user's viewport selection.

## Contract

- Feature-enabled native CLI and TUI sessions translate `--viewport` into the
  bounded `NativeEngineConfig` viewport contract.
- A non-incognito native profile persists Rust-owned web storage under
  `GLASS_CONFIG_HOME/glass/native-profiles/<profile>/storage.json`, or under
  the platform config directory when `GLASS_CONFIG_HOME` is unset.
- `--incognito` leaves `storage_path` unset, so the session remains volatile.
- Profile names use the existing validated profile-name contract. Native
  profile state is intentionally separate from Chromium profile directories.
- Chromium profile and viewport handling remain available only on the
  explicitly selected Chromium backend; no backend fallback is introduced.

## Implementation

`native_config_from_cli` is the single CLI-to-native configuration adapter.
The alternative-runtime dispatcher and the TUI startup path both call it,
eliminating their previous duplicated viewport setup and ensuring profile and
incognito semantics are identical. The native storage owner already creates
the parent directory when it first persists state.

## Tradeoffs and follow-up

Separate native profile storage avoids corrupting or misinterpreting
Chromium/CDP state, but users do not get automatic migration of existing
Chromium cookies or storage. That migration requires an explicit, audited
format bridge. The profile path is also only startup wiring: cross-process
session ownership, origin-wide persistence coverage, quotas, and full
storage/Web IDL conformance remain issue #40 work.

## Verification

- `cargo fmt --all -- --check`
- `cargo check --quiet -p glass-browser --tests --locked`
- `cargo check --quiet -p glass-browser --no-default-features --tests --locked`
- focused `native_cli_config_owns_profile_storage_and_viewport` test
- `git diff --check`

The scoped local checks pass. No remote CI, release, or publication claim is
made by this slice.
