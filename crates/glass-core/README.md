# glass-core

This Rust package builds the `glass-core` terminal coding harness and its
`gc` command alias. Glass uses it as the default resident model-session engine;
the `glass-dev` host owns project access and governed tool execution.
The `gc` alias refers to this Glass Core binary; it does not invoke the
separate GCN product's `gcn` executable.

The source package is part of the root Cargo workspace and shares its version
and lockfile. The product's install workflow, provider setup, authentication,
model selection, and user documentation live in
[`products/glass-core`](../../products/glass-core/README.md). The
[Glass Core product guide](../../docs/glass-core-product.md) explains its
relationship to the rest of the Glass workspace.

This package does not contain `glass-browser-core`, the native browser engine.
