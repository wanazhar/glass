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

3. Copy one reference and its observation revision, then click it explicitly
   with the revision guard. The successful response includes a typed status,
   the revision transition, and bounded verification evidence:

   ```console
   click r1:b123 --expected-revision 1
   ```

   ```json
   {"status":"succeeded","action":"click","previousRevision":1,"currentRevision":2,"revision":2,"target":{"reference":"r1:b123"},"verification":{"revisionDelta":1,"urlChanged":false,"titleChanged":false,"targetChanged":false,"frameChanged":false}}
   ```

   If another page mutation happened first, the same command returns a typed
   `stale_revision` failure. Run `observe` again and retry with the new
   revision; ambiguous targets and failed postconditions are likewise reported
   as structured errors.

The MCP server or either thin client from
[`clients/typescript`](../clients/typescript) and
[`clients/python`](../clients/python). Keep the terminal visible, show the
compact JSON response, and omit credentials, cookies, screenshots, and page
secrets.
