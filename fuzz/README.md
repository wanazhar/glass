# Parser fuzzing

The fuzz targets call Glass parser entry points for:

- MCP framing;
- CDP envelopes;
- accessibility and DOM projection;
- locators and waits;
- normalized URL policy; and
- strict Task Protocol, Web IR, and compiler determinism contracts.

Keep fuzz corpora small. Do not add sensitive data.

Install `cargo-fuzz`. Run a bounded local sweep:

```console
for target in mcp_frame cdp_message ax_dom locator url_policy semantic_contracts; do
  cargo fuzz run "$target" -- -runs=512
done
```

Pull requests run the same deterministic smoke budget. The scheduled workflow
runs each target for two minutes.
