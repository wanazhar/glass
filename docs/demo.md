# 30-second demo recipe

This is a short, reproducible terminal storyboard for a recording. It uses the
same local control-plane flow as the public scorecard and makes no performance
claim.

1. Start one disposable session in the TUI:

   ```console
   glass --incognito --headed
   ```

2. Enter `navigate https://example.com`, then `observe`. The output contains
   bounded page state and revisioned interactive references:

   ```console
   navigate https://example.com
   observe
   ```

3. Copy one reference from that observation and click it explicitly. A stale
   or ambiguous reference fails closed and asks for a fresh observation:

   ```console
   click r1:b123
   ```

The MCP server or either thin client from
[`clients/typescript`](../clients/typescript) and
[`clients/python`](../clients/python). Keep the terminal visible, show the
compact JSON response, and omit credentials, cookies, screenshots, and page
secrets.
