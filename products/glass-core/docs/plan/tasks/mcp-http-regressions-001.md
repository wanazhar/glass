---
id: mcp-http-regressions-001
status: complete
scope: verify and fix HTTP transport interoperability defects
---
# Mcp http regressions

Implementation record. Scope and verification statements describe the recorded checkpoint. See the [current guides](../../INDEX.md) for supported user behavior.

## Contract

The bridge's advertised classic initialize fallback must support negotiated versions, a server-assigned MCP-Session-Id, and successful empty notification responses. New stateless discovery must retain its own metadata/version rules.

SSE requests must return when a matching complete JSON-RPC response event arrives, without waiting for the server to close its stream. Reads remain bounded and respect the request deadline; unrelated notifications and multi-line SSE data are handled safely. No real account/provider calls required.

Confirm defects with deterministic localhost fixtures before implementation, then add regression coverage to smoke-mcp and run independent protocol/PTY checks. Preserve existing legacy stdio and stateless HTTP support. No credential or default installation changes.

The HTTP exception for loopback endpoints must validate literal IP addresses,
not accept arbitrary DNS names beginning with `127.`. Empty or non-object
responses to requests produce a clear protocol error.

## Primary references

- https://modelcontextprotocol.io/specification/2025-11-25/basic/transports
- https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/draft/basic/transports/streamable-http.mdx

## Verification

The HTTP fixture and smoke suite cover session/version headers, empty 202
notifications, incomplete and open SSE events, slow-drip deadlines, response
limits, loopback host validation and malformed replies. Independent targeted
checks and the final 29-suite release run passed. See
[the review](../reviews/oct8-regressions-001-01.md) for binary and log evidence.
