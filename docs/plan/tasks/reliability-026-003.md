# Forbidden-outcome certification gate

Status: complete locally.

The gate requires complete scenario coverage, exact scenario hashes,
independent oracle evidence, complete artifacts, valid platform and budget
metadata, matching terminal state and side-effect counters, and no forbidden
outcomes. Failed, indeterminate, unsupported, and over-budget evidence cannot
certify a release.

The offline CLI surface is:

```console
glass certify release --version 0.2.0 \
  --scenarios scenarios.json --observations observations.json
```

No browser is started by this command. Public release and remote publication
remain outside this local phase.
