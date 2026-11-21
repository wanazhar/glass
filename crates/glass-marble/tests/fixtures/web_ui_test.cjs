const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');
const root = path.join(__dirname, '../../src/static');
const L = require(path.join(root, 'logic.js'));
const people = [{ id: 'marble', name: 'Marble', isDefault: true }, { id: 'a', name: 'Ada' }, { id: 'b', name: 'Ben' }];

test('who answers follows whole-name, case-insensitive host rules', () => {
  assert.deepEqual(L.whoAnswers('hello', people).map(p => p.id), ['marble']);
  assert.deepEqual(L.whoAnswers('@ADA, check this', people).map(p => p.id), ['a']);
  assert.deepEqual(L.whoAnswers('@ada and @ben', people).map(p => p.id), ['a', 'b']);
  assert.deepEqual(L.whoAnswers('@everyone', people).map(p => p.id), ['a', 'b']);
  assert.deepEqual(L.whoAnswers('@everyone @ada', people).map(p => p.id), ['a', 'b']);
  for (const text of ['@adaptive', '@ben-tools', '@ben_2', '@bené', '@unknown']) assert.deepEqual(L.whoAnswers(text, people).map(p => p.id), ['marble']);
  assert.deepEqual(L.whoAnswers('@everyone', [people[0]]), []);
  assert.deepEqual(L.whoAnswers('', []), []);
  assert.equal(L.answerHint('', people), 'Marble will answer.');
  assert.equal(L.answerHint('@everyone', []), 'No Marbles to answer yet.');
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
