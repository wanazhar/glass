const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const test = require('node:test');

const html = fs.readFileSync(require('node:path').join(__dirname, '../../src/static/index.html'), 'utf8');
function transcript(api) {
  const app = { conversationId: 'first', transcript: [], transcriptCursor: 0, transcriptRequest: 0, state: { conversations: [{ id: 'first' }] } };
  const context = vm.createContext({ app, api, activeConversation: () => app.conversationId });
  vm.runInContext(html.slice(html.indexOf('async function syncTranscript()'), html.indexOf('function resetLive(')), context);
  return { app, sync: () => context.syncTranscript() };
}

test('old transcript responses cannot overwrite the newly selected chat', async () => {
  const pending = [];
  const { app, sync } = transcript(() => new Promise((resolve) => pending.push(resolve)));
  const first = sync();
  app.conversationId = 'second';
  const second = sync();
  pending[1]({ entries: [{ text: 'new', at: 2 }], cursor: 2 });
  await second;
  pending[0]({ entries: [{ text: 'old', at: 1 }], cursor: 1 });
  await first;
  assert.equal(app.transcript.length, 1);
  assert.equal(app.transcript[0].text, 'new');
  assert.equal(app.transcriptCursor, 2);
});

test('missing chats are explained and removed, other errors stay visible', async () => {
  let error = Object.assign(new Error('not_found'), { status: 404 });
  const { app, sync } = transcript(async () => { throw error; });
  await sync();
  assert.equal(app.transcriptError, 'This chat no longer exists');
  assert.equal(app.state.conversations.length, 0);
  error = new Error('network failed');
  await sync();
  assert.equal(app.transcriptError, 'Could not load this chat: network failed');
});

test('empty transcripts clear a previous load error', async () => {
  const { app, sync } = transcript(async () => ({ entries: [], cursor: 0 }));
  app.transcriptError = 'old error';
  await sync();
  assert.equal(app.transcriptError, null);
  assert.equal(app.transcript.length, 0);
});
