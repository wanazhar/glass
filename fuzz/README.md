# Parser fuzzing

The five targets drive Glass's production parsing entry points for MCP framing,
CDP envelopes, accessibility/DOM projection, locators/waits, and normalized URL
policy. Keep corpora small and non-sensitive.

Install `cargo-fuzz`, then run a bounded local sweep:

```sh
for target in mcp_frame cdp_message ax_dom locator url_policy; do
  cargo fuzz run "$target" -- -runs=512
done
```

Pull requests run the same deterministic smoke budget. The scheduled workflow
runs each target for two minutes so corpus discoveries accumulate without
making normal CI latency unbounded.
