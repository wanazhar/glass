# Experimental capabilities

Experimental capabilities are intentionally opt-in. They are available for
evaluation, but Glass does not promise a stable API, complete platform
coverage, or unchanged behavior between releases.

## Discover status first

Inspect the negotiated capability manifest without starting a browser:

```console
glass capabilities
```

Check both fields:

- `capabilities` says whether a capability is currently usable;
- `capabilityStatuses` explains why, such as `disabledByPolicy`, `experimental`,
  or `blockedBySecurityGate`.

Clients should use `capabilityStatuses` when deciding whether to offer an
experimental feature. A boolean alone does not explain whether a feature is
disabled by policy, missing a runtime dependency, or blocked for security.

## Enable experimental extensions

Pass the explicit global flag for the process that will use the capability:

```console
# Inspect the resulting manifest
glass --experimental-extensions capabilities

# Start the TUI with the experimental capability negotiated
glass --experimental-extensions tui

# Start MCP with the experimental capability negotiated
glass --experimental-extensions --mcp
```

The flag is not persistent. Omitting it on the next process start disables the
experimental capability again. Glass prints a warning when the flag is used.

On Linux ARM64, the flag reports extensions as
`experimental` only when all of these conditions hold:

- the host is Linux ARM64;
- Bubblewrap is available and can start; and
- the extension uses the sandboxed host path.

On another OS or architecture, or when the sandbox is unavailable, the status
remains `blockedBySecurityGate`. Glass must not silently fall back to an
ordinary unsandboxed subprocess.

## What to expect

Experimental means more than “the code might have bugs.” Expect all of the
following:

- interfaces, manifest fields, and behavior may change in a later release;
- an invocation may fail, time out, or be unavailable on another platform;
- browser and extension behavior may differ from the stable capabilities;
- documentation and compatibility guarantees are limited to the stated
  target and runtime evidence; and
- an experimental capability must not be used as a production security
  boundary.

Extension code is untrusted code. The sandbox reduces its operating-system
authority, but does not make it trusted. Run only extensions you have reviewed,
avoid sensitive browser profiles while evaluating them, and do not provide
secrets merely because an extension declares a host or action permission.
Guarded browser actions remain subject to Glass policy, revision checks, target
permissions, and bounded verification.

## Why a capability remains experimental

A capability can remain experimental when one or more of these are true:

1. its public contract is still evolving;
2. required runtime dependencies are not available everywhere;
3. native security behavior has only been verified on a limited target set;
4. cross-interface compatibility is incomplete; or
5. failure, recovery, migration, or resource-limit behavior needs more
   evidence.

The status is a deliberate compatibility and safety signal, not a claim that
the implementation is unusable. Experimental capabilities can be useful for
local evaluation while remaining unsuitable for general support claims.

## Guidance for future experimental capabilities

Every new experimental capability should provide:

- a default-off configuration or explicit global opt-in;
- a stable capability ID and `experimental` status in the manifest;
- a fail-closed status when its platform or security dependency is missing;
- a visible warning that explains risk and expected instability;
- CLI, TUI, MCP, and library behavior that agrees on the status;
- deterministic tests for opt-in, refusal, and dependency absence; and
- documentation covering enablement, scope, limitations, and disablement.

Do not change a capability from `experimental` to `available` merely because
one local smoke test passes. Promote it only after its contract, security
boundary, applicable target environments, and compatibility evidence justify
the broader claim.
