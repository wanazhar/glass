# glass-mod-api

`glass-mod-api` defines the dependency-free Rust types that Glass extensions
can observe or influence. Glass Core consumes these contracts when loading
mods; the crate is a shared workspace package, not an end-user executable.

The API surface is in [`src/lib.rs`](src/lib.rs) and generated Rust API docs.
Extension authoring and policy are covered by the
[Glass Core extension guide](../../products/glass-core/docs/extending.md) and
[customization governance](../../docs/customization-governance.md).

The zero-dependency boundary is intentional: add new shared contract types to
this crate rather than introducing runtime or provider dependencies here.
