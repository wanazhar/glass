# glass-browser-core

`glass-browser-core` contains the native browser engine and the bounded data
contracts shared with `glass-browser`. Its default feature set contains only
the data contracts and limits. Enable `native-engine` to build the browser
runtime, including rendering, JavaScript, networking, and content workers.

Most users should install or depend on
[`glass-browser`](https://crates.io/crates/glass-browser). It provides browser
sessions, runtime selection, and the Rust browser API. This crate is an
implementation dependency of that package. Its engine modules are private;
the small integration surface used by `glass-browser` is not a promise that
all engine internals are public API. The shared data contracts are documented
in rustdoc.

`glass-browser-core` is not the `glass-core` resident agent engine. They are
separate crates and programs.

See the [published Rust API docs](https://docs.rs/glass-browser-core) when this
crate version is available. The repository's
[browser-core architecture guide](https://github.com/wanazhar/glass/blob/main/docs/architecture/browser-core.md)
describes crate responsibilities, feature behavior, and module ownership.
