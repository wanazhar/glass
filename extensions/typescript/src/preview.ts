import { stringArgument, type Handler } from '@glass-core/extension-sdk';

export const execute: Handler = (args, context) => {
  context.signal.throwIfAborted();
  return {
    text: 'Proposed replacement. No file was written.',
    preview: { title: 'Text preview', before: stringArgument(args, 'before'), after: stringArgument(args, 'after') },
    diagnostics: [{ severity: 'info', message: 'Use the native edit tool when you are ready to apply this change.' }],
  };
};
