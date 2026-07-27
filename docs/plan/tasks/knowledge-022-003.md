---
id: knowledge-022-003
scope: knowledge recognition, freshness, and invalidation assessment
status: completed
depends-on: [knowledge-022-002]
---

# Objective

Assess remembered records against current browser/session scope and fresh
semantic signals before any later knowledge-assisted operation.

# Delivered

- Added `KnowledgeLookupContext` construction from a semantic observation and
  explicit profile, locale, tenant, browser, schema, and policy dimensions.
- Added path-family matching, scope signal reporting, required-landmark checks,
  RFC3339 age checks, and browser-version scope checks.
- Added explicit eligible, out-of-scope, stale, contradicted, and quarantined
  assessment states with bounded conflicts and missing-landmark evidence.
- Enforced RFC3339 timestamps for record provenance and lifecycle history.

Assessment results contain no current target references and cannot authorize a
browser mutation.
