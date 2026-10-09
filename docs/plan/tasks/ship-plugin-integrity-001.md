---
id: ship-plugin-integrity-001
status: in-progress
scope: ship-next-20 item 18 and plugin capability inspection
---

Implement #18 in plugins.rs only, plus dedicated smoke-plugin-integrity.py/tests.
No main/config/ext edits. Use content/path SHA256 receipts and resolved git
revision+manifest version; verify managed installs before loading skills/MCP.
Keep legacy unmanaged plugins compatible with explicit unverified report.
Capabilities live under plugin.json extensions.org.glass-core.capabilities
(filesystem/network/subprocess declarations); inspect/compare bounded JSON
before staged update. Commands plugin verify, plugin update <name> [--accept-permissions]
use saved source, atomic staged replacement, preserve plugins/data, neverreplace
on integrity/install failure. Increased capabilities require explicit flag; print
reviewable diff before refusing. Same-size/mtime-preserved tamper must detect.
Reject escaping symlinks and traversal; no new dependencies beyond ring SHA256.
