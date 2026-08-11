# Tag signing and GitHub verification

The `v0.3.4` annotated tag is cryptographically valid locally with EdDSA key
`C7102B6A568EABDE023F818528E01A5852DB1559`, but GitHub's tag API reports
`verification.verified=false` and reason `unknown_key`. Glass therefore does
not describe that tag as GitHub-verified.

GitHub verifies signed tags only when it can associate the signing public key
and tagger identity with the maintainer account. Before creating `v0.3.5`, the
maintainer must:

1. add the public GPG key containing fingerprint
   `C7102B6A568EABDE023F818528E01A5852DB1559` to the GitHub account that owns
   the tagger identity;
2. ensure the key contains the tagger email and that the same email is verified
   on the GitHub account;
3. configure Git to use that key, create a signed annotated tag, and verify it
   locally with `git tag -v v0.3.5`;
4. push only after explicit approval, then inspect the GitHub tag API.

The release workflow now fails before GitHub Release creation unless the tag
API reports `verification.verified=true`. It records neither local signature
success nor an uploaded key as equivalent to GitHub verification.

An SSH signing key is a practical alternative for future tags if it is added
to GitHub specifically as a signing key and Git is configured for SSH-format
tag signatures. Do not change signing models immediately before release
without first proving a disposable signed tag through the GitHub API.
