// Floating overlays share palette borders; edge-to-edge phone readers stay square.
import { chromium } from 'playwright-core';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
const browser = await chromium.launch({ executablePath: process.env.GC_BROWSER_BINARY || (existsSync('/snap/chromium/current/usr/lib/chromium-browser/chrome') ? '/snap/chromium/current/usr/lib/chromium-browser/chrome' : undefined), headless: true, args: ['--no-sandbox', '--disable-dev-shm-usage'] });
try {
 const html = await readFile(new URL('../src/index.html', import.meta.url), 'utf8');
 const ids = [...html.matchAll(/<dialog\s+id="([^"]+)"/g)].map(match => match[1]);
 assert(ids.includes('commands') && ids.includes('command-view') && ids.includes('preview'));
 const css = await readFile(new URL('../src/style.css', import.meta.url), 'utf8');
 for (const width of [390, 768, 1440]) for (const theme of ['light', 'dark']) for (const mode of ['', 'standalone', 'standalone keyboard-open']) {
  const page = await browser.newPage({ viewport: { width, height: 900 }, reducedMotion: 'reduce' });
  await page.setContent(`<html class="${theme} ${mode}"><body></body></html>`);
  await page.addStyleTag({ content: css });
  const result = await page.evaluate(ids => {
    const styles = [];
    for (const id of [...ids, 'workflow', 'changes', 'teammates', 'git']) {
      const dialog = document.createElement('dialog'); dialog.id = id;
      if (['workflow', 'changes', 'teammates'].includes(id)) dialog.className = 'workflow';
      if (['changes', 'teammates', 'git'].includes(id)) dialog.classList.add('floating-palette');
      const header = document.createElement('header'); header.textContent = 'Overlay title';
      const body = document.createElement('div'); body.style.height = '1200px'; body.textContent = 'Scrollable content';
      dialog.append(header, body); document.body.append(dialog); dialog.showModal();
      const s = getComputedStyle(dialog);
      styles.push({ id, radius: [s.borderTopLeftRadius, s.borderTopRightRadius, s.borderBottomLeftRadius, s.borderBottomRightRadius], border: s.borderTopWidth, color: s.borderTopColor, overflow: s.overflow, width: dialog.getBoundingClientRect().width, height: dialog.getBoundingClientRect().height, x: dialog.getBoundingClientRect().x, y: dialog.getBoundingClientRect().y });
      dialog.close(); dialog.remove();
    }
    return styles;
  }, ids);
  for (const overlay of result) {
    const reader = !['commands', 'changes', 'teammates', 'git'].includes(overlay.id);
    assert.deepEqual(overlay.radius, Array(4).fill(reader ? '0px' : '16px'), `${overlay.id} ${width} ${mode}`);
    assert.equal(overlay.border, reader ? '0px' : '1px');
    if (!reader) assert.equal(overlay.color, result[0].color);
    assert.equal(overlay.overflow, 'hidden');
    if (reader) { assert.equal(overlay.width, width); assert.equal(overlay.height, 900); }
    else { assert(Math.abs(overlay.x + overlay.width / 2 - width / 2) <= 1, overlay.id + ' horizontal centering'); assert(Math.abs(overlay.y + overlay.height / 2 - 450) <= 1, overlay.id + ' vertical centering'); }
  }
  console.log(`PASS ${result.length} overlay borders ${width}px ${theme} ${mode || 'browser'}`);
  await page.close();
 }
} finally { await browser.close(); }
