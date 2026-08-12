# Tag signing and GitHub verification

The `v0.3.5` annotated tag is cryptographically valid with EdDSA key
`C7102B6A568EABDE023F818528E01A5852DB1559`. GitHub's tag API reports
`verification.verified=true`, reason `valid`, and verification time
`2026-08-11T23:44:26Z`. The tag resolves to commit
`3c528689b70396ac5f30367ed89f4d13e3d0ee78`.

The historical `v0.3.4` tag remains locally valid but GitHub reports its
signature as `unknown_key`; Glass does not retroactively describe it as
GitHub-verified.

GitHub verifies signed tags only when it can associate the signing public key
and tagger identity with the maintainer account. The `v0.3.5` release used this
procedure:

1. add the public GPG key containing fingerprint
   `C7102B6A568EABDE023F818528E01A5852DB1559` to the GitHub account that owns
   the tagger identity;
2. ensure the key contains the tagger email and that the same email is verified
   on the GitHub account;
3. configure Git to use that key, create the signed annotated tag, and verify
   it locally with `git tag -v v0.3.5`;
4. push only after explicit approval, then require the GitHub tag API to report
   a valid verified signature before release creation.

The release workflow now fails before GitHub Release creation unless the tag
API reports `verification.verified=true`. It records neither local signature
success nor an uploaded key as equivalent to GitHub verification.

An SSH signing key is a practical alternative for future tags if it is added
to GitHub specifically as a signing key and Git is configured for SSH-format
tag signatures. Do not change signing models immediately before release
without first proving a disposable signed tag through the GitHub API.
