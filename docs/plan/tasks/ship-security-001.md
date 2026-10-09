---
id: ship-security-001
status: pending
depends-on: [ship-terminal-001]
scope: ship-next-20 items 15-19
---

## Objective

Ship opt-in permission profiles, extension confinement/capabilities, plugin
integrity and MCP input requests. Read ../ship-next-20.md. Keep unrestricted
legacy default and two built-in tools; no blanket prompts on existing installs.

## Paths

permissions.rs/config.rs, agent.rs/tools.rs/jobs.rs, sandbox.rs, ext protocol/host/
registry, plugins.rs, MCP bridge.py, modal.rs/main.rs and integration tests.

## Contracts

Permission profiles must enforce filesystem write boundaries at execution, not
claim security from shell regex classification. Unknown extension effects are
refused in read-only mode unless declared and constrained. Workspace-edit permits
workspace/temp writes; approved exceptions are exact commands and session scoped.
Headless cannot silently approve interactive exceptions. Both bash and /bg obey.

Extension isolation is configured separately from model shell policy, inherited
by MCP children; secret environment scrub remains. Inspect capabilities BEFORE
unrestricted startup using a sidecar descriptor (or explicit config declarations),
not only the executable's post-launch handshake. Missing declaration in strict
mode refuses; legacy off remains compatible. Show filesystem/network/process
needs, enforce kernel-supported bounds, and explicitly report port-only egress
and subprocess enforcement limitations. Requested strict policy cannot downgrade.

Plugin locks use SHA256 of file content/path, resolved revision and manifest
version (metadata-only hashes are not integrity). Validate before loading skills
or starting MCP. Preserve plugin persistent data. Staged upgrade computes a
capability diff before atomic replace; explicit acceptance required for new
permissions. Test same-size/mtime-preserved tampering and escaping symlinks.

MCP 2026 input_required routes bounded inputRequests to host. Support flat primitive
form schema/enum/boolean/number and URL modes, review before submit, decline/cancel,
requestState+inputResponses on retry with a new RPC id. Declare actual supported
capabilities. No passwords/API keys in forms; use HTTPS URL consent displaying
host, no automatic navigation. Responses must not be appended to transcript/logs.
Headless returns explicit input-required decline/refusal without waiting forever.
Reference: official MCP 2026-07-28 client/elicitation and multi-round-trip specs.

## Verification

Real OS deny-path fixtures, command-chain and /bg tests, inherited MCP child jail,
redacted env, missing strictcapabilities, plugin tampering/upgrade failure/approval,
mock MCP form/enum/url/cancel/headless paths and PTY at 40x24/20x8. Full final gate.
