# glass-marble

`glass-marble` is a small conversational harness on top of Glass Core. Each
turn runs through `glass-core runtime` over the JSON-RPC host protocol, using
[`glass-runtime-client`](../glass-runtime-client/README.md). It keeps
conversations, transcripts and pending approvals so a chat can resume after a
restart.

- `glass-marble "message" [--conversation ID]` runs one turn and prints the
  reply. Set `GLASS_CORE_BINARY` to use a specific `glass-core` binary.
- `glass-marble serve [--port 4783]` serves the local web UI from
  [`src/static`](src/static).

The library API is in [`src/lib.rs`](src/lib.rs). It is a Cargo workspace
member, not a separate Glass product.
