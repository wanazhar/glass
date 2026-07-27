---
id: knowledge-022-012
scope: hashed target fingerprints and intent evidence
status: completed
depends-on: [knowledge-022-011]
---

# Objective

Allow eligible persistent target knowledge to explain a current intent
candidate without supplying a selector, reference, or authorization.

# Delivered

- Added stable target-fingerprint digests over role, hashed name, input type,
  region, and intent purpose.
- Added target-fingerprint record construction with bounded non-sensitive
  payloads; current references, route handles, and names are excluded.
- Added historical evidence to intent results only when the current fresh
  candidate independently exists and an eligible fingerprint matches.
- Kept historical evidence out of confidence promotion and guarded execution
  authorization.
