---
id: semantic-020-008
scope: semantic fixture corpus and scorecard
status: completed
depends-on: [semantic-020-007]
---

# Objective

Keep deterministic semantic payload canaries that exercise each bounded
payload shape and privacy boundary.

# Delivered

Added four version-one fixtures covering summary, interactive, detailed, and
raw levels. `examples/semantic_scorecard.rs` validates strict parsing,
canonical round trips, duplicate names, payload bounds, and the absence of
secret field names. The scorecard is offline and deterministic.
