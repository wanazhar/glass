import { chromium } from 'playwright-core';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
const browser = await chromium.launch({ executablePath: process.env.GC_BROWSER_BINARY || (existsSync('/snap/chromium/current/usr/lib/chromium-browser/chrome') ? '/snap/chromium/current/usr/lib/chromium-browser/chrome' : undefined), headless: true, args: ['--no-sandbox', '--disable-dev-shm-usage'] });
try {
  for (const width of [390, 768, 1440]) for (const theme of ['light', 'dark']) {
    const page = await browser.newPage({ viewport: { width, height: 844 } });
    await page.setContent(`<html class="${theme}"><head></head><body><main id="command-result" class="command-data" style="max-width:760px;padding:20px;margin:auto"></main></body></html>`);
    await page.addStyleTag({ content: await readFile(new URL('../dist/style.css', import.meta.url), 'utf8') });
    await page.addScriptTag({ type: 'module', content: (await readFile(new URL('../src/command-results.js', import.meta.url), 'utf8')).replace('export function', 'window.renderCommandData = function') });
    await page.waitForFunction(() => window.renderCommandData);
    await page.evaluate(() => renderCommandData(document.querySelector('main'), { commands: Array.from({ length: 71 }, (_, i) => ({ command: `gc command-${i}`, purpose: `Description ${i}` })), topics: ['files', 'learning'], human_actions: [{ command: '/status', purpose: 'Connection details' }] }, { command: 'guidance' }));
    assert.equal(await page.locator('.command-catalogue article').count(), 72);
    assert.equal(await page.locator('details').count(), 0);
    await page.getByRole('searchbox').fill('command-23');
    assert.equal(await page.locator('.command-catalogue article:visible').count(), 1);
    await page.evaluate(() => { const root = document.querySelector('main'); root.replaceChildren(); renderCommandData(root, { requests: 3, reported_prompt_total: 12345, reported_output: 234, reported_cache_hit_ratio: null, known_cost_subtotal_usd: null, cost_complete: false, requests_with_usage: 2, requests_without_usage: 1, scope: 'Recent requests only', fs_scan: { hits: 0 }, cache_note: 'Missing usage is never assumed to be zero.' }, { command: 'usage' }); });
    assert.equal(await page.locator('.command-metric').count(), 5);
    assert.match(await page.locator('.command-metrics').innerText(), /Not reported/);
    assert.equal(await page.locator('details[open]').count(), 0);
    assert.equal(await page.getByText('File search cache', { exact: true }).isVisible(), false);
    await page.getByText('More details', { exact: true }).click();
    assert.equal(await page.getByText('File search cache', { exact: true }).isVisible(), true);
    await page.evaluate(() => { const root = document.querySelector('main'); root.replaceChildren(); renderCommandData(root, { connection: 'Kilo via saved API key', model: 'kilo-auto/free', provider: 'openai', security: { allow: [], profile: 'unrestricted', env: '', ports: [8080, 443] } }, { command: 'status' }); });
    assert.equal(await page.locator('details').count(), 0);
    assert.match(await page.locator('main').innerText(), /kilo-auto\/free/);
    assert(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.evaluate(() => { const root = document.querySelector('main'); root.replaceChildren(); renderCommandData(root, Array.from({ length: 20 }, (_, i) => ({ name: `Task ${i}`, id: i + 50, title: 'Keep title', status: 'pending', output: 'Line one\nLine two' })), { command: 'tasks' }); });
    assert.equal(await page.locator('.command-result-list > article').count(), 20);
    assert.match(await page.locator('.command-list-item').first().innerText(), /50/);
    assert.match(await page.locator('.command-list-item').first().innerText(), /Keep title/);
    assert.equal(await page.locator('.command-stat-wide pre').first().innerText(), 'Line one\nLine two');
    await page.getByRole('searchbox').fill('Task 19');
    assert.equal(await page.locator('.command-result-list > article:visible').count(), 1);
    await page.evaluate(() => { const root = document.querySelector('main'); root.replaceChildren(); renderCommandData(root, { diff: 'x'.repeat(70000) }, { markdown: (target, source) => target.textContent = source }); });
    assert.match(await page.locator('main').innerText(), /Preview limited to 64K/);
    await page.evaluate(() => { const root = document.querySelector('main'); root.replaceChildren(); renderCommandData(root, [{ record: 9, excerpt: 'Quoted text' }], { command: 'quote', onSelect: item => window.chosenRecord = item.record }); });
    await page.getByText('Use this excerpt', { exact: true }).click();
    assert.equal(await page.evaluate(() => window.chosenRecord), 9);
    if (width === 390 && theme === 'light') {
      await page.screenshot({ path: '/tmp/gc-command-status-mobile.png', fullPage: true });
      await page.evaluate(() => { const root = document.querySelector('main'); root.replaceChildren(); renderCommandData(root, Object.fromEntries(Array.from({ length: 20000 }, (_, i) => [`field_${i}`, i]))); });
      assert(await page.locator('.command-stat').count() <= 3000);
      assert.match(await page.locator('main').innerText(), /Display limit reached/);
      await page.addScriptTag({ type: 'module', content: (await readFile(new URL('../src/activity.js', import.meta.url), 'utf8')).replaceAll('export function ', 'window.').replace(/window\.(\w+)\(/g, 'window.$1 = function(') });
      await page.waitForFunction(() => window.createToolActivity);
      assert(await page.evaluate(() => {
        const item = createToolActivity({ name: 'bash', command: 'find /home/ubuntu -name image.png', saved: false, preview() {}, copy() {}, activity() {} });
        item.startedAt = performance.now() - 15000; refreshToolTime(item);
        if (item.waitHint.hidden || !item.waitHint.textContent.includes('broad find')) return false;
        finishToolActivity(item, false); return item.waitHint.hidden;
      }));
    }
    console.log(`PASS readable command views ${width}px ${theme}`);
    await page.close();
  }
} finally { await browser.close(); }
