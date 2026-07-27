---
id: semantic-020-003
scope: observation levels and bounded region expansion
status: completed
depends-on: [semantic-020-002]
---

# Objective

Make the semantic contract level-aware while keeping every payload bounded
and revision-scoped.

# Delivered

Summary observations contain page and region summaries only. Interactive,
structured, detailed, and raw levels add bounded target references, visible
text, compact accessibility nodes, and a bounded raw accessibility projection
in that order. Form values and deep DOM remain outside this contract.

Added `BrowserSession::semantic_expand_region`, which requires the caller's
revision to match a fresh observation and returns one region with omission
metadata for the rest of the page. Stale revisions and unknown region IDs are
reported as typed validation errors.
