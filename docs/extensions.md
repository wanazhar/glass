# Extensions

The extension capability is disabled by default. It is available only through
an explicit experimental opt-in when the required native sandbox is available.

See [Experimental capabilities](experimental-capabilities.md) for the full
opt-in, status, safety, and compatibility guidance.

## Manifest

An extension manifest declares:

- an extension ID;
- an API version;
- a capability;
- exact allowed hosts; and
- bounded allowed actions.

The validator rejects wildcard hosts, JavaScript evaluation, raw CDP,
undeclared browser commands, and undeclared mutations.

Load manifests without executing extension code:

```rust
ExtensionRegistry::load_dir
```

The host confines entrypoints to its configured root. It checks declared
capabilities, hosts, and actions. It limits each request and response to
256 KiB. It permits four concurrent calls. It stops a call after five seconds.

The host accepts one result or one error for each request. It rejects
oversized values and sensitive field names, including cookies, tokens,
passwords, secrets, and authorization data.

Registering a manifest does not execute code.

## Guarded browser actions

An extension does not receive a browser handle.

```rust
ExtensionHost::invoke_guarded
```

accepts a bounded revision-pinned action result. Glass then performs supported
click, type, clear, check, uncheck, and select actions through the core
`BrowserSession` methods.

Glass keeps policy checks, target resolution, revision checks, verification, and
effect recording in the core runtime.

## Sandbox

Enable the experimental capability explicitly:

```console
glass --experimental-extensions capabilities
```

This prints `experimental` only when the native sandbox is detected. Glass
prints a warning because extension code is untrusted and the interface may
break. A missing sandbox remains `blockedBySecurityGate`; the opt-in never
falls back to an ordinary unsandboxed subprocess.

Use the explicit sandbox entry point:

```rust
ExtensionHost::invoke_sandboxed
```

On Linux, the host requires `bubblewrap`. On macOS, it uses the system
sandbox profile. If the sandbox is not available, the call fails. Glass does
not fall back to the ordinary subprocess host.

The sandbox does not provide browser handles. It does not bypass policy.

## First-party fixtures

Two reference extensions are in `crates/glass-browser/extensions/first-party/`. They test:

- manifest validation;
- exact host and action permissions; and
- the bounded host protocol; and
- cold start, clean process exit, and restart for each reference extension.

They are fixtures. Glass does not load or enable them automatically.

Read [glass-extension-v1.schema.json](schema/glass-extension-v1.schema.json).
The lifecycle test starts each reference extension twice. The host waits for
each process to exit before it returns. This does not make an extension
trusted. Redaction, native sandbox, and cross-transport checks must also pass
before Glass enables the capability.

To verify the native sandbox in a target environment, run:

```console
GLASS_EXTENSION_SANDBOX_E2E=1 \
GLASS_EXTENSION_SANDBOX_KIND=linux-bubblewrap \
cargo test --lib extensions::tests::sandboxed_reference_extensions_pass_native_gate --locked
```

This verifies the native sandbox on Linux ARM64 only. The capability remains
`blockedBySecurityGate` for any target that has not passed its own native
sandbox check.
