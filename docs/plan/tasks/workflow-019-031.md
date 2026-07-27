---
id: workflow-019-031
scope: effect-marker duplicate-dispatch protection
status: completed
depends-on: [workflow-019-030]
---

# Objective

Consume declarative effect markers at the safe retry boundary.

# Delivered

After a failure proven to occur before dispatch, Glass evaluates `beforeRetry`
once when present. A matched marker commits the step through the normal
verified/output/committed states without dispatching the action again. Marker
evaluation errors stop the run; no marker path permits an automatic
post-dispatch retry.
