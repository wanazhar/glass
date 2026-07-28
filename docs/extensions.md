# Extensions

Glass 0.2 defines an extension manifest, permission boundary, and a bounded
process host. `ExtensionRegistry::load_dir` loads JSON manifests without
executing them; `ExtensionHost::invoke` can then run one declared entrypoint
request at a time.

An extension must declare an ID, API version, capability, exact host list, and
bounded action list. Wildcard hosts, `evaluate`, raw CDP, arbitrary browser
commands, and undeclared mutations are rejected by the manifest validator.
The host confines entrypoints below its configured root, allows only declared
capabilities, exact hosts, and actions, caps each request/response at 256 KiB,
and terminates calls after five seconds. A host response must use the versioned
one-result/one-error protocol. Registering metadata alone never executes code.

The current host is not a native-code sandbox. It does not receive browser
handles and cannot yet participate in the guarded executor, policy decisions,
revision checks, redaction, or certification flow. Therefore the negotiated
`extensions` capability remains disabled.

Two reference extensions live under `extensions/first-party/`. They exercise
the manifest, exact-host/action permissions, and bounded host protocol for an
extraction transform and an intent-evidence pack. They are fixtures for the
extension design, not automatically loaded or enabled by the runtime.

`ExtensionHost::invoke_sandboxed` is an explicit opt-in boundary. On Linux it
requires `bubblewrap`; on macOS it uses the system sandbox profile. If neither
boundary is available, the call fails instead of falling back to the ordinary
bounded subprocess host. The sandboxed path still does not grant browser
handles or bypass policy; guarded-executor integration remains a separate gate.

See [glass-extension-v1.schema.json](schema/glass-extension-v1.schema.json).
Native sandboxing, lifecycle supervision, guarded executor adapters, and
extension conformance remain required before the capability can be enabled.
