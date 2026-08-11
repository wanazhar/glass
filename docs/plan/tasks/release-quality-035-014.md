# Release notes, signing truth, and migration

Status: Local workflow complete; GitHub key enrollment and exact-tag gates pending

## Implementation

- Replaced `gh release create --generate-notes` with a validated repository
  release source and exact tag/commit/workflow/crates publication record.
- Required major features, breaking changes, security, installation/migration,
  known limitations, and validation evidence in every release body.
- Blocked tag publication validation and GitHub Release creation unless
  GitHub's tag API reports the annotated signature as verified.
- Recorded the current `v0.3.4` API result (`unknown_key`) without converting
  local cryptographic verification into a GitHub-verification claim.
- Added the complete 0.3.4-to-0.3.5 migration guide and synchronized the
  Unreleased changelog.

## Maintainer action

The public GPG key and matching verified tagger email must be associated with
the maintainer's GitHub account before the v0.3.5 tag is created. This local
checkout cannot mutate account signing-key settings. The exact tag must then
pass GitHub verification, CI, fuzz/security, package, publish-dry-run,
install/upgrade, ordered crates publication, and release-record checks.

No tag, push, publication, GitHub Release, or issue mutation was performed by
this task.
