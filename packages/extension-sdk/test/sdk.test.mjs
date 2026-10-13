import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { once } from 'node:events';
import { renderResult, runExtension } from '../dist/index.js';
import { PassThrough } from 'node:stream';
function fixture(t) {
  const child = spawn(process.execPath, [new URL('./fixture.mjs', import.meta.url).pathname], { stdio: ['pipe', 'pipe', 'pipe'] });
  const messages = [], waiters = []; let stderr = '';
  child.stderr.on('data', data => { stderr += data; });
  createInterface({ input: child.stdout }).on('line', line => {
    const message = JSON.parse(line), waiter = waiters.shift();
    if (waiter) waiter(message); else messages.push(message);
  });
  t.after(() => { if (child.exitCode === null) child.kill(); });
  const next = () => messages.length ? Promise.resolve(messages.shift()) : new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error(`Missing response: ${stderr}`)), 3000);
    waiters.push(message => { clearTimeout(timeout); resolve(message); });
  });
  const send = message => child.stdin.write(JSON.stringify(message) + '\n');
  let id = 0;
  const request = async (type, fields = {}) => { const rid = ++id; send({ type, id: rid, ...fields }); const reply = await next(); assert.equal(reply.id, rid); return reply; };
  const hello = () => request('hello', { protocol: 1, workspace: '/tmp/sdk', model: 'test' });
  const tool = mode => request('run_tool', { name: 'fixture', call_id: 'origin', lineage: { origin: 'origin', depth: 0, path: ['origin'] }, arguments: JSON.stringify({ mode }) });
  const close = async () => { const exit = once(child, 'exit'); send({ type: 'goodbye', id: ++id }); const [code] = await exit; assert.equal(code, 0, stderr); assert.match(stderr, /disposed/); };
  return { child, next, send, request, hello, tool, close };
}
test('fast handshake, lazy initialization and persistent state', async t => {
  const f = fixture(t), manifest = await f.hello();
  assert.equal(manifest.protocol, 1); assert.equal(manifest.authorization_required, true);
  assert.equal(manifest.tools[0].concurrency, 'serial');
  assert.equal((await f.request('command', { name: 'fixture-status', args: '' })).output, 'loads=0, calls=0');
  const result = await f.tool('preview');
  assert.match(result.output, /loads=1, calls=1/); assert.match(result.output, /### Diagnostics/); assert.match(result.output, /a.ts:2/);
  assert.match((await f.tool('preview')).output, /loads=1, calls=2/);
  await f.close();
});
test('argument errors, unknown tools and authorization exceptions remain actionable', async t => {
  const f = fixture(t); await f.hello();
  for (const arguments_ of ['{', '[]', 'null', '{}']) {
    assert.match((await f.request('run_tool', { name: 'fixture', arguments: arguments_ })).output, /INVALID_ARGUMENT/);
  }
  assert.match((await f.tool('error')).output, /TEST_ERROR.*useful failure/);
  assert.match((await f.request('run_tool', { name: 'missing', arguments: '{}' })).output, /UNKNOWN_TOOL/);
  const denied = await f.request('authorize', { name: 'broken', arguments: '{}' });
  assert.equal(denied.allow, false); assert.match(denied.reason, /gate failed/);
  assert.equal((await f.request('authorize', { name: 'deny', arguments: '{}' })).allow, false);
  assert.match((await f.tool('preview')).output, /loads=1/);
  await f.close();
});
test('host tool_result hooks are not mistaken for nested replies', async t => {
  const f = fixture(t); await f.hello();
  const result = await f.request('tool_result', { call_id: 'origin', name: 'read', body: 'preserved output' });
  assert.equal(result.body, 'preserved output');
  await f.close();
});
test('nested host replies correlate by call ID and state remains outside model text', async t => {
  const f = fixture(t); await f.hello();
  f.send({ type: 'run_tool', id: 20, name: 'fixture', arguments: '{"mode":"nested"}' });
  const first = await f.next(), second = await f.next();
  assert.equal(first.type, 'call_tool'); assert.equal(second.type, 'call_tool');
  assert.notEqual(first.call_id, second.call_id);
  f.send({ type: 'call_tool_result', call_id: second.call_id, body: 'B' });
  f.send({ type: 'call_tool_result', call_id: first.call_id, body: 'A' });
  const append = await f.next(); assert.equal(append.type, 'append_custom'); assert.equal(append.data, 'AB'); assert.equal(append.kind, 'sdk-fixture/notes');
  f.send({ type: 'custom_result', call_id: append.call_id, ok: true });
  const read = await f.next(); assert.equal(read.type, 'read_custom'); assert.equal(read.kind, 'sdk-fixture/notes');
  f.send({ type: 'custom_result', call_id: read.call_id, ok: true, entries: ['saved', 'AB'] });
  assert.deepEqual(await f.next(), { id: 20, output: 'saved,AB' });
  await f.close();
});
test('oversized output becomes a bounded error and process remains reusable', async t => {
  const f = fixture(t); await f.hello();
  assert.match((await f.tool('large')).output, /OUTPUT_LIMIT/);
  assert.match((await f.tool('preview')).output, /loads=1/);
  await f.close();
});
test('shutdown aborts an in-flight handler and disposes once', async t => {
  const f = fixture(t); await f.hello();
  f.send({ type: 'run_tool', id: 90, name: 'fixture', arguments: '{"mode":"wait"}' });
  await new Promise(resolve => setTimeout(resolve, 25));
  await f.close();
});
test('oversized and malformed host frames fail promptly', async t => {
  for (const value of ['bad json\n', 'x'.repeat(4 * 1024 * 1024 + 1)]) {
    const f = fixture(t); await f.hello();
    const exit = once(f.child, 'exit'); f.child.stdin.on('error', () => {}); f.child.stdin.write(value);
    const [code] = await exit; assert.notEqual(code, 0);
  }
});
test('native preview fences cannot be closed by source and diagnostic rows stay intact', () => {
  const result = renderResult({ preview: { after: '```\nx\n```', language: 'ts\nBAD' }, diagnostics: [{ severity: 'info', message: 'a|b\nc' }] });
  assert.match(result, /````tsBAD/); assert.match(result, /a\\\|b c/); assert.equal(renderResult('plain'), 'plain');
});

test('cleanup still runs when sending the failure reply also fails', async () => {
  const input = new PassThrough(); let disposed = 0;
  const running = runExtension({ name: 'broken-output', version: '1', dispose() { disposed++; } }, {
    input, output: { write() { throw new Error('broken output'); } }, errors: { write() { return true; } },
  });
  input.write('{"id":1,"type":"hello","protocol":1}\n');
  await assert.rejects(running, /broken output/);
  assert.equal(disposed, 1);
});
