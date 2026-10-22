# Negotiated mixed tools for resident Glass sessions

## Current behavior

Before this change, Glass negotiated runtime protocol v1 and opened a
`toolMode: "host"` session with the complete Glass tool catalog. Core replaced
its native tool set and routed every model call back through Glass. The Pi
backend was already a separate, explicitly selected legacy path.

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

## Implemented contract

Protocol v2 negotiation is implemented in `initialize`. The request and response carry an
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

The mixed catalog is an explicit subset of Glass services: browser and Git,
workspace/project inspection, read-only editor and language-server context,
non-writing editor proposals, and Glass project-task APIs. It excludes direct
editor writes, command execution, process controls, and every `glass.agent.*`
tool. Those paths either duplicate Core's coding/provider/session ownership or
would keep Glass in the direct coding-edit loop.

## Compatibility and legacy engine

Protocol compatibility is negotiated, not inferred from package version. The
reported package version is informational. A Core runtime that cannot advertise the required v2 mixed contract is rejected
with an actionable message; Glass does not fall back to exclusive host mode.
The Pi SDK adapter remains available only through the existing explicit
`GLASS_AGENT_HARNESS=pi` legacy selection and is outside the v2 contract. It
does not receive duplicate provider credentials from Glass.

## Verification record

- [x] Add a mixed-tool capability and explicit protocol negotiation while
      preserving v1 host/local behavior.
- [x] Add Core agent support for a combined native + host integration catalog,
      deterministic ownership-based dispatch, collision checks, and session
      metadata.
- [x] Restrict Glass's registered catalog to Glass-owned integrations; retain
      Glass-side policy and evidence for those calls only.
- [x] Make Glass require the negotiated capability and give a clear
      compatibility diagnostic; keep Core provider/model/auth authoritative.
- [x] Extend fixture and package tests for native-only, Glass-only, mixed,
      collision, unsupported-version, cancellation, and unchanged legacy modes.
- [x] Update current harness/runtime docs and product-facing setup language;
      keep historical records scoped as historical.
- [x] Run documentation validators and focused `glass-core` and `glass-dev`
      tests, format/clippy on changed Rust, and inspect the final diff for
      accidental protocol, TUI, or unrelated workspace changes.

## Contract decisions

Protocol v2 is an exact version request with an explicit required capability;
there is no downgrade path. Ownership is determined by exact membership in
the registered host catalog, with `glass.*` reserved for Glass integrations.
Core rejects namespace violations and collisions with Core tools or extensions
before session creation. The runtime reports `mixedTools` only when v2 was
negotiated.

Focused verification passed: `cargo test -p glass-core --locked mixed`,
`cargo test -p glass-dev --lib core_runtime --locked`,
`cargo clippy -p glass-core -p glass-dev --all-targets --locked -- -D warnings`,
`cargo fmt --all -- --check`, and the documentation depth, coverage,
release-truth, and version-sync validators. Core tests cover native and
registered Glass calls, ownership metadata, collisions, and protocol gating;
Glass adapter tests cover v1 rejection, missing capability rejection,
version-independent negotiation, exact tool allowlisting, and mixed-session
resume behavior.
