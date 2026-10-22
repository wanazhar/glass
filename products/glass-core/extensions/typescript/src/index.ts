import { runExtension } from '@glass-core/extension-sdk';

await runExtension({
  name: 'typescript-demo',
  version: '0.3.14',
  tools: [{
    definition: {
      name: 'ts_preview',
      description: 'Preview a text replacement without changing files.',
      concurrency: 'parallel',
      schema: { type: 'object', properties: { before: { type: 'string' }, after: { type: 'string' } }, required: ['before', 'after'], additionalProperties: false },
    },
    load: () => import('./preview.js'),
  }],
  commands: [{ name: 'ts-demo', description: 'Show a native TypeScript extension preview', execute: () => ({
    text: 'TypeScript extensions use GC’s native renderer.',
    preview: { title: 'Text preview', language: 'typescript', before: 'const color = "blue";', after: 'const color = "violet";' },
    diagnostics: [{ severity: 'info', message: 'Preview only; no files changed.' }],
  }) }],
});
