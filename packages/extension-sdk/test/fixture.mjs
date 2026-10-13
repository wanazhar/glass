import { runExtension, stringArgument, ToolError } from '../dist/index.js';
let loads = 0, calls = 0;
await runExtension({
  name: 'sdk-fixture', version: '1.0.0',
  tools: [{ definition: { name: 'fixture', description: 'SDK test', schema: { type: 'object' }, concurrency: 'serial' },
    load: async () => {
      loads++;
      return { execute: async (args, ctx) => {
        calls++;
        const mode = stringArgument(args, 'mode');
        if (mode === 'error') throw new ToolError('TEST_ERROR', 'A useful failure');
        if (mode === 'large') return 'x'.repeat(4 * 1024 * 1024);
        if (mode === 'wait') { await new Promise(resolve => ctx.signal.addEventListener('abort', resolve, { once: true })); return 'stopped'; }
        if (mode === 'nested') {
          const [first, second] = await Promise.all([ctx.callTool('bash', { command: 'cat a' }), ctx.callTool('bash', { command: 'cat b' })]);
          await ctx.appendCustom('notes', first + second);
          return (await ctx.readCustom('notes')).join(',');
        }
        ctx.log('handled');
        return { text: `loads=${loads}, calls=${calls}, workspace=${ctx.workspace}, call=${ctx.callId}`,
          preview: { before: 'a', after: 'b', language: 'typescript' },
          diagnostics: [{ severity: 'warning', file: 'a.ts', line: 2, message: 'check | this\nline' }] };
      }};
    } }],
  commands: [{ name: 'fixture-status', description: 'Local state', execute: () => `loads=${loads}, calls=${calls}` }],
  authorize: (name) => { if (name === 'broken') throw new Error('gate failed'); return { allow: name !== 'deny' }; },
  dispose: () => { process.stderr.write('disposed\n'); },
}, { timeoutMs: 150 });
