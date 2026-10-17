// Audit every registered palette route against an isolated host stub; never run mutations.
import { chromium } from 'playwright-core';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { resolve, extname } from 'node:path';
const root = resolve('web/dist');
const { commands } = JSON.parse(await readFile(new URL('./fixtures/commands.json', import.meta.url)));
const streams = new Set(), sent = [], errors = [];
let holdStatus = false;
const heldStatus = [];
const selected = new Set((process.env.GC_PALETTE_COMMANDS || '').split(',').filter(Boolean));
const included = command => !selected.size || selected.has(command.name);
function result(name) {
  if (name === 'usage') return { format: 'json', body: { requests: 9, reported_prompt_total: 45813, reported_output: 672, reported_cache_hit_ratio: 0.823, known_cost_subtotal_usd: null, cost_complete: false, scope: 'Recent requests only.' } };
  if (name === 'quote') return { format: 'json', body: [{ record: 9, excerpt: 'Quoted text' }] };
  if (name === 'context' || name === 'skill-doctor') return { format: 'text', body: 'Instructions\n\nLine one\nLine two' };
  if (['todo', 'review', 'recap'].includes(name)) return { format: 'markdown', body: '## Current state\n\n- Pending item' };
  return { format: 'json', body: { status: 'ready', items: [{ id: 7, name: 'Example item', status: 'pending', output: 'Line one\nLine two', owner: 'User' }], empty: [] } };
}
const server = createServer(async (req, res) => {
  const url = new URL(req.url, 'http://localhost'), path = url.pathname;
  if (path === '/api/events') {
    res.writeHead(200, { 'Content-Type': 'text/event-stream' }); streams.add(res);
    res.write(`data: ${JSON.stringify({ reset: true, control: { can_write: true, web_workflows: 1, commands }, active: false })}\n\n`);
    req.on('close', () => streams.delete(res)); return;
  }
  if (path.startsWith('/api/')) {
    let data = {};
    if (path === '/api/status') data = { workspace: '/tmp', web_workflows: 1, profile: 'workspace-edit' };
    if (path === '/api/threads') data = { threads: [{ session: 'fixture', title: 'Palette audit', workspace: '/tmp', workspace_assigned: true, workspace_available: true }], projects: [] };
    if (path === '/api/history') data = { records: [], before: 0 };
    if (path === '/api/commands') data = { commands };
    if (path === '/api/command' || path === '/api/send') {
      let text = ''; for await (const chunk of req) text += chunk;
      const body = JSON.parse(text);
      if (path === '/api/command') data = result(body.name);
      else if (body.message.t === 'command') {
        const m = body.message; sent.push(m);
        if (!(holdStatus && m.name === 'status')) assert.deepEqual(m.values, {}, `Opening /${m.name} must not execute a mutation or model request`);
        const deliver = (fail = false) => {
          for (const stream of streams) stream.write(`data: ${JSON.stringify({ events: [{ t: 'command_result', request_id: m.request_id, ...(fail ? { error: 'Fixture unavailable' } : { result: result(m.name) }) }] })}\n\n`);
        };
        if (holdStatus && m.name === 'status') heldStatus.push(deliver); else deliver();
      }
    }
    res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(data)); return;
  }
  try {
    const file = resolve(root, '.' + (path === '/' ? '/index.html' : path));
    if (!file.startsWith(root + '/')) throw Error('bad path');
    res.writeHead(200, { 'Content-Type': ({ '.js': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.woff2': 'font/woff2', '.webmanifest': 'application/manifest+json' })[extname(file)] || 'application/octet-stream' });
    res.end(await readFile(file));
  } catch { res.writeHead(404).end(); }
});
await new Promise(r => server.listen(0, '127.0.0.1', r));
const browser = await chromium.launch({ executablePath: process.env.GC_BROWSER_BINARY || (existsSync('/snap/chromium/current/usr/lib/chromium-browser/chrome') ? '/snap/chromium/current/usr/lib/chromium-browser/chrome' : undefined), headless: true, args: ['--no-sandbox', '--disable-dev-shm-usage'] });
try {
 for (const width of [390, 1440]) for (const colorScheme of ['light', 'dark']) {
  const context = await browser.newContext({ viewport: { width, height: 900 }, colorScheme, serviceWorkers: 'block' });
  const page = await context.newPage(); page.on('pageerror', e => errors.push(String(e)));
  await page.goto(`http://127.0.0.1:${server.address().port}/?thread=fixture`);
  await page.waitForFunction(() => document.querySelector('#ownership').textContent === 'Release control');
  for (const command of commands.filter(c => c.portable && included(c))) {
    holdStatus = command.name === 'status';
    if (holdStatus) await page.locator('#command-result').evaluate(n => n.classList.add('prose'));
    await page.locator('#prompt').fill('/');
    await page.locator('#commands').waitFor({ state: 'visible' });
    await page.locator('#command-search').fill(command.name);
    const choice = page.locator('.command-choice').filter({ has: page.locator('strong', { hasText: new RegExp('^/' + command.name + '(?:\\s|$)') }) });
    await choice.click();
    await page.locator('#command-view').waitFor({ state: 'visible' });
    assert.equal(await page.locator('#command-title').textContent(), '/' + command.name);
    if (holdStatus) {
      const waitPending = async () => {
        for (let n = 0; n < 200 && !heldStatus.length; n++) await new Promise(r => setTimeout(r, 10));
        assert(heldStatus.length, 'Expected held status request');
      };
      const geometry = () => page.evaluate(() => ['#command-view', '#command-form', '.command-actions'].map(selector => {
        const r = document.querySelector(selector).getBoundingClientRect(); return [r.top, r.height, r.width];
      }));
      await waitPending();
      await page.locator('#command-result[aria-busy=true] .command-skeleton').waitFor();
      assert.equal(await page.locator('#command-result').getAttribute('class'), 'command-data');
      await page.locator('#command-view').evaluate(n => Promise.all(n.getAnimations().map(a => a.finished.catch(() => {}))));
      const before = await geometry();
      assert.equal(await page.locator('#command-view > footer').count(), 0);
      assert(await page.locator('.command-actions').evaluate(n => n.getBoundingClientRect().height <= 44));
      heldStatus.shift()();
      await page.locator('#command-result[aria-busy=false]').waitFor();
      assert.deepEqual(await geometry(), before, 'Loading must not move controls, action row or dialog');
      await page.evaluate(() => window.previousResult = document.querySelector('#command-result').firstElementChild);
      await page.locator('#command-form .command-run').click();
      await waitPending();
      assert(await page.evaluate(() => document.querySelector('#command-result').firstElementChild === window.previousResult));
      assert.equal(await page.locator('.command-skeleton').count(), 0, 'Refresh keeps the previous results');
      heldStatus.shift()(true);
      await page.locator('#command-result[aria-busy=false]').waitFor();
      assert(await page.evaluate(() => document.querySelector('#command-result').firstElementChild === window.previousResult), 'Failed refresh preserves the previous result');
      assert.match(await page.locator('#command-status').innerText(), /Fixture unavailable/);
      assert.deepEqual(await geometry(), before, 'Failed refresh must not resize the shell');
      await page.locator('#command-close').click();
      await page.locator('#prompt').fill('/');
      await page.locator('#commands').waitFor({ state: 'visible' });
      await page.locator('#command-search').fill('status');
      await page.locator('.command-choice').filter({ has: page.locator('strong', { hasText: /^\/status(?:\s|$)/ }) }).click();
      await waitPending();
      heldStatus.shift()(true);
      await page.locator('#command-result[aria-busy=false]').waitFor();
      assert.equal(await page.locator('.command-skeleton').count(), 0);
      assert.match(await page.locator('#command-result').innerText(), /Results could not load/);
      holdStatus = false;
    }
    await page.waitForFunction(() => !document.querySelector('#command-status').dataset.working || document.querySelector('#command-status').dataset.working === 'false');
    for (const input of await page.locator('#command-form .command-field:visible input, #command-form .command-field:visible select, #command-form .command-field:visible textarea').all()) {
      assert.equal(await input.evaluate(n => getComputedStyle(n.parentElement).display), 'flex', 'Form fields must not inherit result grids');
    }
    if (command.name === 'usage' && width === 390 && colorScheme === 'light') await page.screenshot({ path: '/tmp/gc-compact-command-status.png' });
    if (command.name === 'quote') {
      await page.getByText('Use this excerpt', { exact: true }).click();
      assert.equal(await page.locator('#command-form [name=args]').inputValue(), '9');
      assert.equal(await page.locator('#command-form [name=text]').inputValue(), 'Quoted text');
    }
    if (command.name === 'model') {
      assert.equal(await page.locator('#command-form [name=args]').isVisible(), false);
      await page.locator('#command-form [name=action]').selectOption('set');
      assert.equal(await page.locator('#command-form [name=args]').isVisible(), true);
    }
    if (['constraints', 'pacing'].includes(command.name)) assert.equal(await page.locator('#command-form [name=args]').evaluate(n => n.tagName), 'INPUT');
    if (command.name === 'neuralnets') assert.equal(await page.locator('#command-form [name=kind]').evaluate(n => n.tagName), 'SELECT');
    await page.locator('#command-form').evaluate(form => {
      const action = form.elements.namedItem('action');
      if (action?.tagName !== 'SELECT') return;
      for (const option of action.options) {
        action.value = option.value; action.dispatchEvent(new Event('change'));
        for (const label of form.querySelectorAll('.command-field')) {
          const input = label.querySelector('input,select,textarea');
          if (input && input.disabled !== label.hidden) throw Error('Field visibility and submission disagree');
        }
      }
    });
    assert.equal(await page.locator('#command-view').evaluate(n => getComputedStyle(n).borderTopLeftRadius), command.name === 'git' ? '16px' : '0px');
    assert.equal(await page.locator('#command-result .command-section').count(), 0);
    assert(await page.locator('#command-view').evaluate(n => n.scrollWidth <= n.clientWidth + 1));
    await page.locator('#command-close').click();
  }
  // Exercise all local routes too; each starts from an independent page state.
  for (const command of commands.filter(c => !c.portable && included(c))) {
    await page.goto(`http://127.0.0.1:${server.address().port}/?thread=fixture`);
    await page.waitForFunction(() => document.querySelector('#ownership').textContent === 'Release control');
    await page.locator('#prompt').fill('/');
    await page.locator('#commands').waitFor({ state: 'visible' });
    await page.locator('#command-search').fill(command.name);
    const choice = page.locator('.command-choice').filter({ has: page.locator('strong', { hasText: new RegExp('^/' + command.name + '(?:\\s|$)') }) });
    assert.equal(await choice.count(), 1);
    await choice.click();
    if (command.primary) assert(await page.locator('#commands').isVisible());
    if (command.name === 'help') {
      assert.equal(await page.locator('#commands').isVisible(), false);
      assert.equal(await page.locator('#preview-title').innerText(), 'Help');
      assert.match(await page.locator('#preview-body').innerText(), /Using Glass/);
      const requestsBefore = sent.length;
      await page.getByRole('searchbox', { name: 'Search help commands' }).fill('/usage');
      const usage = page.locator('.help-command:visible').filter({ has: page.locator('strong', { hasText: /^\/usage$/ }) });
      assert.equal(await usage.count(), 1);
      await usage.click();
      assert.equal(await page.locator('#command-title').innerText(), '/usage');
      assert.equal(sent.length, requestsBefore, 'Help and usage inspection do not send model work');
      await page.locator('#command-close').click();
      await page.locator('#prompt').fill('/help');
      await page.locator('#prompt').press('Enter');
      await page.locator('#preview').waitFor({ state: 'visible' });
      assert.equal(await page.locator('#preview-title').innerText(), 'Help');
    }
    if (['outputs', 'thinking'].includes(command.name)) assert(await page.locator('#details').isVisible());
    if (command.name === 'editor') assert(await page.locator('#draft-editor').isVisible());
    if (command.name === 'animation') assert(await page.locator('#animations').isVisible());
    if (command.name === 'tools') assert(await page.evaluate(() => document.documentElement.classList.contains('compact-tools')));
  }
  assert.deepEqual(errors, []);
  console.log(`PASS ${commands.length} palette entries; ${commands.filter(c => c.portable && included(c)).length} native forms/results ${width}px ${colorScheme}`);
  await context.close();
 }
} finally { await browser.close(); for (const stream of streams) stream.end(); await new Promise(r => server.close(r)); }
