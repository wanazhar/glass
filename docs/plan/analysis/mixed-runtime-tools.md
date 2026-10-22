# Negotiated mixed tools for resident Glass sessions

## Current behavior

Glass starts `glass-core runtime`, negotiates runtime protocol v1, then opens a
`toolMode: "host"` session with the complete Glass tool catalog. In Core,
`Agent::with_host_tools` replaces the native tool set and routes every model
tool call back through Glass. This makes Core's standalone coding tools
unavailable in a resident Glass session and gives Glass ownership of provider
model-tool execution. The Pi backend remains a separate, explicitly selected
legacy path.

## Ownership decision

The resident session must combine two catalogs under one Core-owned model and
session loop:

| Owner | Responsibilities |
|---|---|
| Glass Core | Provider selection and credentials, model configuration, session transcript, native coding tools, and their execution policy |
| Glass | Browser, Git, project/workspace, editor, daemon, and other Glass integration tools; Glass-side authorization and evidence for those integrations |

Core's standalone `local` mode remains unchanged. The generic `host` mode also
remains available for other hosts that intentionally own the entire tool
surface. Glass will use the new `mixed` mode and register only its integration
catalog. It must not pass provider credentials to Glass or silently substitute
an older host-only runtime. An explicit model selection may be sent to Core;
provider and credential authority remain in Core.

## Contract proposal

Add protocol v2 negotiation to `initialize`. The request and response carry an
explicit protocol version and capability set; Glass requires the advertised
mixed-tools capability and reports a clear incompatible-runtime error if it is
missing. Keep v1 available to existing clients and keep its `host` and `local`
semantics unchanged. New Glass sessions request `toolMode: "mixed"`, a
versioned host catalog, and an ownership namespace. Glass integration names
must be namespaced (`glass.*`); Core-native tools retain their existing names.
Reject invalid schemas, duplicate/colliding names, or unsupported catalog
versions before creating a session.

The Core agent presents both native and registered Glass schemas to the same
provider session. Calls to native tools execute in Core under Core's existing
policy; calls in the negotiated Glass namespace produce `tool/execute` frames
and continue through Glass authorization, evidence, cancellation, and result
handling. Session metadata reports both catalogs and their owners. Existing
host-only execution and host-neutral standalone operation remain supported.

## Compatibility and legacy engine

Protocol compatibility is negotiated, not inferred from package version. A
Core runtime that cannot advertise the required v2 mixed contract is rejected
with an actionable message; Glass does not fall back to exclusive host mode.
The Pi SDK adapter remains available only through the existing explicit
`GLASS_AGENT_HARNESS=pi` legacy selection and is outside the v2 contract. It
does not receive duplicate provider credentials from Glass.

## Implementation checklist

- [ ] Add a mixed-tool capability and explicit protocol negotiation while
      preserving v1 host/local behavior.
- [ ] Add Core agent support for a combined native + host integration catalog,
      deterministic ownership-based dispatch, collision checks, and session
      metadata.
- [ ] Restrict Glass's registered catalog to Glass-owned integrations; retain
      Glass-side policy and evidence for those calls only.
- [ ] Make Glass require the negotiated capability and give a clear
      compatibility diagnostic; keep Core provider/model/auth authoritative.
- [ ] Extend fixture and package tests for native-only, Glass-only, mixed,
      collision, unsupported-version, cancellation, and unchanged legacy modes.
- [ ] Update current harness/runtime docs and product-facing setup language;
      keep historical records scoped as historical.
- [ ] Run documentation validators and focused `glass-core` and `glass-dev`
      tests, format/clippy on changed Rust, and inspect the final diff for
      accidental protocol, TUI, or unrelated workspace changes.

## Open implementation details

Confirm whether v2 accepts only an exact requested version or uses a supported
version range. The preferred minimum is a deterministic exact version plus
explicit capability response, to avoid silent downgrade. Choose a stable owner
field and namespace rule before code, and ensure extensions cannot shadow
reserved host integration names.
