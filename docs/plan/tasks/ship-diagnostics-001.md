---
id: ship-diagnostics-001
status: in-progress
scope: ship-next-20 items 7 and 20
---

Implement doctor.rs and performance script/CI artifacts for contracts 7 and20
in ../ship-next-20.md. Own new doctor module, scripts/smoke-doctor.py,
scripts/check-performance.py and perf CI workflow only. Main/config integration
will happen after reliability merges; provide exact hookup requirements and
avoid editing existing shared main/config. Tests in module; pure read-only,
redacted checks and JSON output. No credentials/real provider probes by default.
