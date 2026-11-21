# glass-marble

`glass-marble` is a durable conversational harness that talks to the
standalone `glass-core` runtime through `glass-runtime-client`. It is a
Cargo workspace package, not a separately published end-user product.

The `glass-marble` command runs one turn against a `glass-core` process,
or serves a local product UI with `glass-marble serve`. Set
`GLASS_CORE_BINARY` to the engine path, or keep `glass-core` on `PATH`.

The runtime protocol and host responsibilities are documented in the
[runtime reference](../../products/glass-core/docs/reference/runtime.md)
and the [Glass Core resident harness guide](../../docs/glass-core-harness.md).
