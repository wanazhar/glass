const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const vm = require('node:vm');
const root = path.join(__dirname, '../../src/static');
const L = require(path.join(root, 'logic.js'));
const people = [{ id: 'marble', name: 'Marble', isDefault: true }, { id: 'a', name: 'Ada' }, { id: 'b', name: 'Ben' }];

test('text block resets retract the previous live answer without losing reasoning', () => {
  const state = { sequence: 0, live: new Map(), approvalChats: new Map() };
  const onEvent = appFunction('function onEvent(', 'function connect()', { s: state, L, drawSoon() {}, refreshSoon() {} }, 'onEvent');
  let sequence = 0;
  const send = (kind, text) => onEvent({ type: kind, data: JSON.stringify({ sequence: ++sequence, conversationId: 'chat', taskId: 'task', kind, data: { text } }) });
  send('text', 'Planning.');
  assert.equal(state.live.get('task').text, 'Planning.');
  send('text_reset', ''); send('reasoning', 'Planning.'); send('text', 'Answer.');
  assert.equal(state.live.get('task').text, 'Answer.');
  assert.equal(state.live.get('task').reasoning, 'Planning.');
  send('text_reset', '');
  assert.equal(state.live.get('task').text, '');
});

function appFunction(start, end, context, name) {
  const source = fs.readFileSync(path.join(root, 'app.js'), 'utf8');
  return vm.runInNewContext(source.slice(source.indexOf(start), source.indexOf(end, source.indexOf(start))) + ';' + name, context);
}

test('tasks retain their named owner before a chat exists and older tasks still resolve', () => {
  const owner = appFunction('function taskPerson(', 'function activityPage(', {
    person: id => people.find(person => person.id === id),
    personForChat: id => id === 'ben-chat' ? people[2] : id === 'default-chat' ? people[0] : undefined,
  }, 'taskPerson');
  assert.equal(owner({ marbleId: 'a', conversationId: 'recovered-failed' }), people[1]);
  assert.equal(owner({ conversationId: 'ben-chat' }), people[2]);
  assert.equal(owner({ conversationId: 'default-chat' }), people[0]);
  assert.equal(owner({}), undefined);
});

test('who answers follows whole-name, case-insensitive host rules', () => {
  assert.deepEqual(L.whoAnswers('hello', people).map(p => p.id), []);
  assert.deepEqual(L.whoAnswers('@ADA, check this', people).map(p => p.id), ['a']);
  assert.deepEqual(L.whoAnswers('@ada and @ben', people).map(p => p.id), ['a', 'b']);
  assert.deepEqual(L.whoAnswers('@everyone', people).map(p => p.id), ['marble', 'a', 'b']);
  assert.deepEqual(L.whoAnswers('@everyone @ada', people).map(p => p.id), ['a', 'marble', 'b']);
  assert.deepEqual(L.whoAnswers('@Marble take the lead @everyone', people).map(p => p.id), ['marble', 'a', 'b']);
  assert.deepEqual(L.whoAnswers('@Marble and @Ada help', people).map(p => p.id), ['marble', 'a']);
  for (const text of ['@adaptive', '@ben-tools', '@ben_2', '@bené', '@unknown']) assert.deepEqual(L.whoAnswers(text, people).map(p => p.id), []);
  assert.deepEqual(L.whoAnswers('@everyone', [people[0]]), [people[0]]);
  assert.deepEqual(L.whoAnswers('', []), []);
  assert.equal(L.answerHint('', people), 'The host picks who can help.');
  assert.equal(L.answerHint('@everyone', people), 'Asks everyone.');
});

test('status keeps stream, request failure and engine health independent', () => {
  let state = { stream: false, requestFailed: false, engine: false };
  assert.equal(L.statusLabel(state), 'Reconnecting…');
  state = L.reduceStatus(state, { type: 'open' });
  state = L.reduceStatus(state, { type: 'request', ok: false });
  assert.equal(L.statusLabel(state), 'Connected');
  state = L.reduceStatus(state, { type: 'close' });
  assert.equal(L.statusLabel(state), "Can't reach the host");
  state = L.reduceStatus(state, { type: 'request', ok: true });
  assert.equal(L.statusLabel(state), 'Reconnecting…');
  state = L.reduceStatus(state, { type: 'health', health: { runtime: { health: 'failed' } } });
  state = L.reduceStatus(state, { type: 'open' });
  assert.equal(L.statusLabel(state), 'Engine stopped');
  state = L.reduceStatus(state, { type: 'health', health: { runtime: { health: 'ok' } } });
  assert.equal(L.statusLabel(state), 'Connected');
  assert.deepEqual(L.reduceStatus(state, { type: 'operation_error' }), state);
});

test('routes decode ids and preserve info navigation', () => {
  assert.deepEqual(L.route('#/m/ada%3A1/info'), { screen: 'info', id: 'ada:1' });
  assert.deepEqual(L.route('#/m/marble'), { screen: 'chat', id: 'marble' });
  assert.deepEqual(L.route('#/m/%zz'), { screen: 'missing' });
  assert.deepEqual(L.route('#/everyone/thread-1'), { screen: 'everyone', id: 'thread-1' });
  assert.deepEqual(L.route('#/everyone/%zz'), { screen: 'missing' });
  for (const screen of ['everyone', 'marbles', 'activity', 'settings', 'new']) assert.equal(L.route('#/' + screen).screen, screen);
  assert.equal(L.route('').screen, 'everyone');
});

test('look defaults and personality counters respect host values', () => {
  assert.equal(L.nextHue([]), 210);
  assert.equal(L.nextHue([{ hue: 210 }, { hue: 35 }]), 150);
  assert.equal(L.nextHue(L.hues.map(hue => ({ hue }))), 210);
  assert.equal(L.characterCount(''), 0);
  assert.equal(L.characterCount('A😀'), 2);
  assert.equal(L.characterCount('😀'.repeat(2000)), 2000);
});

test('markdown supports paragraphs, lists, inline and unclosed code blocks', () => {
  assert.deepEqual(L.markdown(''), []);
  const blocks = L.markdown('Hello **Ada** and `code`.\n\n- one\n- two\n\n1. ordered\n\n```js\nconst x = 1;');
  assert.deepEqual(blocks.map(b => b.tag), ['p', 'ul', 'ol', 'pre']);
  assert.equal(blocks[0].content[1].tag, 'strong');
  assert.equal(blocks[1].items.length, 2);
  assert.equal(blocks[3].text, 'const x = 1;');
});

test('model HTML, URLs and attributes remain inert text', () => {
  const attack = '<img src=x onerror="alert(1)"><script>alert(2)</script> [click](javascript:alert(3))';
  const blocks = L.markdown(attack);
  assert.deepEqual(blocks, [{ tag: 'p', content: [{ tag: 'text', text: attack }] }]);
  assert.equal(L.markdown('```\n' + attack + '\n```')[0].text, attack);
  const app = fs.readFileSync(path.join(root, 'app.js'), 'utf8');
  assert.ok(!/innerHTML|outerHTML|insertAdjacentHTML|document\.write\(/.test(app));
});

test('Marble status attributes attention and work to the right chat', () => {
  const person = { id: 'a', conversationId: 'chat-a', state: 'idle' };
  const tasks = [{ conversationId: 'chat-a', status: 'running', prompt: 'Check tests' }];
  assert.equal(L.marbleStatus(person, [], []), 'Idle');
  assert.equal(L.marbleStatus(person, tasks, []), 'Working on Check tests');
  assert.equal(L.marbleStatus(person, tasks, [{ owner: 'a' }]), 'Needs you');
  assert.equal(L.marbleStatus(person, tasks, [{ owner: 'b' }]), 'Working on Check tests');
  assert.equal(L.marbleStatus({ ...person, lastError: 'connection closed' }, [], []), "Couldn't finish: connection closed");
});

test('per-agent task ids never make the default Marble work for Ada', () => {
    const tasks = [{ id: 'a', owner: 'prism', marbleId: 'ada', conversationId: 'chat-a', status: 'running', prompt: 'Research' }];
    assert.equal(L.marbleStatus({ id: 'marble', isDefault: true, activeTaskIds: [], state: 'idle' }, tasks, []), 'Idle');
    assert.equal(L.marbleStatus({ id: 'ada', activeTaskIds: ['a'], state: 'working' }, tasks, []), 'Working on Research');
});

test('failure rows show the plain message and Retry but never diagnostics', () => {
    const render = appFunction('function entryNode(', 'function retry(', {
        tasks: () => [{ id: 'failed-task' }], retry: () => 'Retry', el: (tag, style, ...children) => children
    }, 'entryNode');
    const node = render({ kind: 'failure', taskId: 'failed-task', text: "Couldn't get a reply from the model. Try again.", failure: { category: 'provider_rejected', retryable: true, diagnostics: 'PRIVATE DIAGNOSTICS' } });
    assert.deepEqual(Array.from(node), ["Couldn't get a reply from the model. Try again.", 'Retry']);
    assert.ok(!JSON.stringify(node).includes('PRIVATE'));
});

test('visible interface copy does not expose internal vocabulary', () => {
  const html = fs.readFileSync(path.join(root, 'index.html'), 'utf8').replace(/<[^>]*>/g, ' ');
  const app = fs.readFileSync(path.join(root, 'app.js'), 'utf8');
  const visible = [...app.matchAll(/\bt\((['"])((?:\\.|(?!\1).)*)\1\)/g)].map(m => m[2]);
  assert.ok(visible.length > 100, 'the copy marker must cover the UI');
  const banned = /\b(prism|dispatch|swarm|lane|roster|assistant|control)\b/i;
  for (const text of [html, ...visible]) assert.ok(!banned.test(text), text);
});

test('quiet-hour fields round trip clock times and reject invalid input', () => {
  assert.equal(L.minuteTime(0), '00:00');
  assert.equal(L.minuteTime(1320), '22:00');
  assert.equal(L.minuteTime(1439), '23:59');
  for (const minute of [0, 1, 420, 720, 1439]) assert.equal(L.timeMinute(L.minuteTime(minute)), minute);
  for (const invalid of ['', '9:00', '24:00', '12:60', 'every:1h']) assert.equal(L.timeMinute(invalid), null);
});

test('recent activity selects the latest twenty settled turns without changing state', () => {
  assert.deepEqual(L.recentTasks([]), []);
  const input = Array.from({ length: 25 }, (_, at) => ({ at, status: 'completed' }));
  input.push({ at: 100, status: 'running' }, { at: 101, status: 'queued' });
  const result = L.recentTasks(input);
  assert.equal(result.length, 20);
  assert.equal(result[0].at, 24);
  assert.equal(result[19].at, 5);
  assert.equal(input[0].at, 0);
  assert.equal(L.recentTasks([{ status: 'failed' }])[0].status, 'failed');
});

test('notification results retain each channel success and failure, including empty results', () => {
  assert.deepEqual(L.deliveryLines(null), []);
  assert.deepEqual(L.deliveryLines({ delivered: [] }), []);
  assert.deepEqual(L.deliveryLines({ delivered: [
    { channel: 'webhook', ok: true, detail: 'HTTP 200' },
    { channel: 'apns', ok: false, detail: 'Connection refused' },
  ] }), [
    { ok: true, text: 'webhook · Delivered · HTTP 200' },
    { ok: false, text: 'apns · Not delivered · Connection refused' },
  ]);
});

test('routine schedules use words and preserve the host minimum interval', () => {
  assert.equal(L.scheduleLabel('daily:09:00'), 'Every day at 9:00');
  assert.equal(L.scheduleLabel(' DAILY:23:05 '), 'Every day at 23:05');
  assert.equal(L.scheduleLabel('every:15m'), 'Every 15 minutes');
  assert.equal(L.scheduleLabel('every:60s'), 'Every minute');
  assert.equal(L.scheduleLabel('every:1s'), 'Every minute');
  assert.equal(L.scheduleLabel('every:2h'), 'Every 2 hours');
  assert.equal(L.scheduleLabel('every:24h'), 'Every day');
  assert.equal(L.scheduleLabel(''), 'Not scheduled');
  for (const spec of ['every:0m', 'every:-2h', 'daily:24:00', 'daily:09:60', 'invalid']) assert.equal(L.scheduleLabel(spec), 'Custom schedule');
});

test('next runs use local calendar days, including month and year boundaries', () => {
  const now = new Date(2026, 11, 31, 20).getTime();
  assert.match(L.relativeRun(new Date(2026, 11, 31, 21).getTime(), now, 'en-US'), /^Today, 9:00/);
  assert.match(L.relativeRun(new Date(2027, 0, 1, 9).getTime(), now, 'en-US'), /^Tomorrow, 9:00/);
  assert.match(L.relativeRun(new Date(2027, 0, 2, 9).getTime(), now, 'en-US'), /Jan 2, 2027/);
  for (const at of [0, null, undefined, 'invalid', Infinity]) assert.equal(L.relativeRun(at, now), 'Not scheduled');
});

test('the stylesheet uses only the product type scale', () => {
  const css = fs.readFileSync(path.join(root, 'app.css'), 'utf8');
  for (const match of css.matchAll(/font-size:\s*(\d+)px/g)) assert.ok([12, 13, 15, 17, 22].includes(Number(match[1])), match[0]);
});

test('opaque message parts use their supplied safe Markdown fallback', () => {
  const component = { type: 'future-component', payload: { html: '<script>unsafe()</script>' } };
  const fallback = '**Summary** <img src=x onerror=unsafe()>';
  assert.deepEqual(L.markdown(L.messagePart(component, fallback)), [{ tag: 'p', content: [
    { tag: 'strong', text: 'Summary' }, { tag: 'text', text: ' <img src=x onerror=unsafe()>' },
  ] }]);
  for (const part of [component, {}, null, undefined]) {
    assert.doesNotThrow(() => L.messagePart(part));
    assert.deepEqual(L.markdown(L.messagePart(part)), []);
  }
  assert.equal(L.messagePart(component, { html: '<img src=x onerror=unsafe()>' }), '');
  assert.equal(L.messagePart('Existing **text**', 'Ignored fallback'), 'Existing **text**');
});

test('handoff instructions add a period only when terminal punctuation is missing', () => {
  const handoff = { fromName: 'Ada', toName: 'Ben', status: 'asked' };
  for (const ending of ['.', '!', '?']) assert.equal(L.handoffLine({ text: 'Old doubled punctuation..', handoff: { ...handoff, instruction: 'Check the workspace' + ending + '  ' } }), 'Ada asked Ben to Check the workspace' + ending);
  assert.equal(L.handoffLine({ handoff: { ...handoff, instruction: 'Check the workspace' } }), 'Ada asked Ben to Check the workspace.');
  assert.equal(L.handoffLine({ handoff: { ...handoff, instruction: '' } }), 'Ada asked Ben.');
  assert.equal(L.handoffLine({ text: 'Earlier handoff' }), 'Earlier handoff');
  assert.equal(L.handoffLine(null), '');
});

test('finished handoffs summarize the first line or sentence without changing the full reply', () => {
  const reply = 'Here is the outline:\n\n- What changed\n- What we verified';
  const entry = { handoff: { toName: 'Ben', status: 'finished', summary: reply } };
  assert.equal(L.handoffLine(entry), 'Ben finished: Here is the outline:');
  assert.equal(entry.handoff.summary, reply);
  for (const marker of ['-', '*', '+', '1.', '2)']) assert.equal(L.handoffSummary('\n' + marker + ' **What changed**\n- More detail'), 'What changed');
  assert.equal(L.handoffSummary('All checks passed! More detail follows.'), 'All checks passed!');
  assert.equal(L.handoffSummary('Release 1.2 is ready. More detail follows.'), 'Release 1.2 is ready.');
  const long = L.handoffSummary('🟢'.repeat(150));
  assert.equal(Array.from(long).length, 120);
  assert.ok(long.endsWith('…'));
  for (const empty of ['', '\n  \n', null, undefined, {}]) assert.equal(L.handoffSummary(empty), '');
  assert.equal(L.handoffLine({ handoff: { toName: 'Ben', status: 'finished', summary: '' } }), 'Ben finished.');
});

test('a dirty info panel belongs only to the routed Marble', () => {
  let dirty = false;
  const panel = { dataset: {}, contains: () => false, querySelector: () => dirty, replaceChildren: (...children) => { panel.marble = children.at(-1).id; } };
  const main = { contains: () => false, querySelector: () => null };
  const s = { route: { screen: 'info', id: 'ada' } };
  const context = { s, innerWidth: 1440, document: { activeElement: {}, body: { classList: { toggle() {} } } },
    $: id => id === 'info-panel' ? panel : main, current: () => ({ id: s.route.id }),
    renderRail() {}, header() {}, renderTimeline() {}, composer() {}, icon() {}, t: value => value,
    chatRoute: id => id, link: () => ({ setAttribute() {} }), infoPage: person => person };
  const render = appFunction('function render()', 'async function refresh()', context, 'render');
  render();
  assert.equal(panel.marble, 'ada');
  dirty = true;
  render();
  assert.equal(panel.marble, 'ada');
  s.route.id = 'ben';
  render();
  assert.equal(panel.marble, 'ben', 'dirty Ada fields must not be saved from Ben’s panel');
});

test('named chat messages use the shared turn intake, including the first memory command', async () => {
  for (const conversationId of ['', 'ada-chat']) {
    const requests = [];
    const context = { s: { route: { screen: 'chat' }, generation: 1 }, $: () => ({ value: 'Remember this: prefer concise notes' }),
      current: () => ({ id: 'ada', isDefault: false }), conversation: () => conversationId,
      running: () => null, defaultChats: () => [], updateComposer() {}, action: async (...args) => requests.push(args) };
    const send = appFunction('async function send()', 'function section(', context, 'send');
    await send();
    assert.equal(requests[0][1], '/api/turns');
    assert.equal(requests[0][2].marbleId, 'ada');
    assert.equal(requests[0][2].conversationId || '', conversationId);
  }
});

test('a restarted stream resets stale cursors without duplicating normal reconnect text', () => {
  const s = { sequence: 500, live: new Map([['old', {}]]), approvalChats: new Map() };
  const onEvent = appFunction('function onEvent(event)', 'function connect()', { s, L, refreshSoon() {}, drawSoon() {} }, 'onEvent');
  const emit = (type, payload) => onEvent({ type, data: JSON.stringify(payload) });
  emit('hello', { cursor: 0 });
  emit('text', { sequence: 1, conversationId: 'new-chat', taskId: 'new-task', data: { text: 'New host text' } });
  assert.equal(s.live.get('new-task')?.text, 'New host text');
  assert.equal(s.live.has('old'), false);
  emit('hello', { cursor: 1 });
  emit('text', { sequence: 1, taskId: 'new-task', data: { text: 'New host text' } });
  assert.equal(s.live.get('new-task').text, 'New host text');
  emit('hello', { cursor: 'invalid' });
  assert.equal(s.sequence, 1);
});
