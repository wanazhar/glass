# Extensions

Glass 0.2 defines an extension manifest and permission boundary, but does not
load extension code. This keeps the current runtime honest while leaving a
versioned contract for future hosts.

An extension must declare an ID, API version, capability, exact host list, and
bounded action list. Wildcard hosts, `evaluate`, raw CDP, arbitrary browser
commands, and undeclared mutations are rejected by the manifest validator.
The registry stores metadata only; registering an entrypoint never executes
it and cannot weaken Glass policy, revision checks, redaction, or budgets.

See [glass-extension-v1.schema.json](schema/glass-extension-v1.schema.json).
Dynamic loading, sandboxing, lifecycle isolation, and extension conformance
tests remain separate work before the `extensions` capability can be enabled.
