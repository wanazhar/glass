# glass-browser-core

`glass-browser-core` owns the reusable browser-engine implementation and the
bounded data contracts shared with `glass-browser`. The default feature set
contains only the lightweight serialized contracts and limit definitions.
Enable `native-engine` to compile the native runtime and its rendering,
JavaScript, networking, and content-worker dependencies.

Most users should depend on [`glass-browser`](https://crates.io/crates/glass-browser),
which provides browser sessions and compatibility paths. This crate is the
engine implementation boundary and is intended for integrations that need to
embed or inspect the engine directly. Engine APIs are evolving with the
browser capability contract; stable shared DTOs are documented in rustdoc.

See the repository's [browser-core architecture guide](https://github.com/wanazhar/glass/blob/main/docs/architecture/browser-core.md)
for crate responsibilities, feature behavior, and module ownership.
