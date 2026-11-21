(function (root) {
  'use strict';
  const named = people => people.filter(p => !p.isDefault && p.id !== 'marble');
  function mentioned(text, name) {
    const needle = '@' + name.toLowerCase();
    let rest = String(text).toLowerCase();
    for (let i = rest.indexOf(needle); i !== -1; i = rest.indexOf(needle)) {
      rest = rest.slice(i + needle.length);
      if (!/^[\p{L}\p{N}_-]/u.test(rest)) return true;
    }
    return false;
  }
  function whoAnswers(text, people) {
    if (mentioned(text, 'everyone')) return named(people);
    const chosen = named(people).filter(p => mentioned(text, p.name));
    return chosen.length ? chosen : people.filter(p => p.isDefault || p.id === 'marble');
  }
  function answerHint(text, people) {
    const chosen = whoAnswers(text, people).map(p => p.name);
    return chosen.length ? chosen.join(' and ') + ' will answer.' : 'No Marbles to answer yet.';
  }
  function reduceStatus(state, event) {
    switch (event.type) {
      case 'open': return { ...state, stream: true };
      case 'close': return { ...state, stream: false };
      case 'request': return { ...state, requestFailed: !event.ok };
      case 'health': return { ...state, engine: event.health?.runtime?.health !== 'ok' };
      default: return state;
    }
  }
  function statusLabel(state) {
    if (state.engine) return 'Engine stopped';
    if (state.stream) return 'Connected';
    return state.requestFailed ? "Can't reach the host" : 'Reconnecting…';
  }
  function route(hash) {
    const match = /^#\/m\/([^/]+)(\/info)?$/.exec(hash);
    if (match) { try { return { screen: match[2] ? 'info' : 'chat', id: decodeURIComponent(match[1]) }; } catch (_) { return { screen: 'missing' }; } }
    const screen = hash.replace(/^#\//, '');
    return { screen: ['everyone', 'marbles', 'activity', 'settings', 'new'].includes(screen) ? screen : 'everyone' };
  }
  function inline(text) {
    return String(text).split(/(`[^`\n]+`|\*\*[^*\n]+\*\*)/g).filter(Boolean).map(part =>
      part.startsWith('`') && part.endsWith('`') ? { tag: 'code', text: part.slice(1, -1) } :
        part.startsWith('**') && part.endsWith('**') ? { tag: 'strong', text: part.slice(2, -2) } : { tag: 'text', text: part });
  }
  function messagePart(part, fallback) {
    return typeof part === 'string' ? part : typeof fallback === 'string' ? fallback : '';
  }
  function markdown(text) {
    const blocks = []; let paragraph = []; let code = null; let list = null;
    const flush = () => { if (paragraph.length) blocks.push({ tag: 'p', content: inline(paragraph.join('\n')) }); paragraph = []; list = null; };
    for (const line of String(text || '').replace(/\r\n/g, '\n').split('\n')) {
      if (/^\s*```/.test(line)) { flush(); if (code !== null) { blocks.push({ tag: 'pre', text: code.join('\n') }); code = null; } else code = []; continue; }
      if (code !== null) { code.push(line); continue; }
      if (!line.trim()) { flush(); continue; }
      const item = /^\s*(?:([-*])|\d+\.)\s+(.+)$/.exec(line);
      if (item) { if (paragraph.length) flush(); const tag = item[1] ? 'ul' : 'ol'; if (!list || list.tag !== tag) { list = { tag, items: [] }; blocks.push(list); } list.items.push(inline(item[2])); }
      else { list = null; paragraph.push(line); }
    }
    flush(); if (code !== null) blocks.push({ tag: 'pre', text: code.join('\n') }); return blocks;
  }
  function marbleStatus(person, tasks, approvals) {
    const task = tasks.find(t => t.conversationId === person.conversationId && t.status === 'running') ||
      (person.isDefault ? tasks.find(t => t.owner === 'agent' && t.status === 'running') : null);
    if (task && approvals.some(a => a.conversationId === task.conversationId || a.owner === person.id)) return 'Needs you';
    if (person.state === 'working' || task) return 'Working on ' + (task?.prompt || 'a reply');
    if (person.lastError) return "Couldn't finish: " + person.lastError;
    return 'Idle';
  }
  const hues = [210, 35, 150, 280, 340, 85, 185, 245];
  const nextHue = people => hues.find(h => !people.some(p => Number(p.hue) === h)) ?? hues[0];
  const characterCount = text => Array.from(text).length;
  const minuteTime = minute => [Math.floor((Number(minute) || 0) / 60) % 24, (Number(minute) || 0) % 60].map(n => String(n).padStart(2, '0')).join(':');
  const timeMinute = time => { const match = /^(\d{2}):(\d{2})$/.exec(time); return match && Number(match[1]) < 24 && Number(match[2]) < 60 ? Number(match[1]) * 60 + Number(match[2]) : null; };
  const recentTasks = tasks => tasks.filter(task => !['running', 'queued'].includes(task.status)).slice().sort((a, b) => (b.at || 0) - (a.at || 0)).slice(0, 20);
  const deliveryLines = result => (result?.delivered || []).map(item => ({ ok: !!item.ok, text: [item.channel, item.ok ? 'Delivered' : 'Not delivered', item.detail].filter(Boolean).join(' · ') }));
  function scheduleLabel(spec) {
    const text = String(spec || '').trim().toLowerCase();
    if (!text) return 'Not scheduled';
    const daily = /^daily:\s*(\d+)\s*:\s*(\d+)$/.exec(text);
    if (daily && Number(daily[1]) < 24 && Number(daily[2]) < 60) return 'Every day at ' + Number(daily[1]) + ':' + String(Number(daily[2])).padStart(2, '0');
    const every = /^every:\s*(\d+)\s*([smhd])$/.exec(text);
    if (!every || Number(every[1]) < 1 || !Number.isSafeInteger(Number(every[1]))) return 'Custom schedule';
    const seconds = Math.max(60, Number(every[1]) * ({ s: 1, m: 60, h: 3600, d: 86400 }[every[2]]));
    const [unit, divisor] = [['day', 86400], ['hour', 3600], ['minute', 60], ['second', 1]].find(([, divisor]) => seconds % divisor === 0);
    const count = seconds / divisor;
    return count === 1 ? 'Every ' + unit : 'Every ' + count + ' ' + unit + 's';
  }
  function relativeRun(at, now = Date.now(), locale) {
    if (!at || !Number.isFinite(Number(at))) return 'Not scheduled';
    const date = new Date(Number(at)); const today = new Date(now);
    if (Number.isNaN(date.getTime())) return 'Not scheduled';
    const day = value => Date.UTC(value.getFullYear(), value.getMonth(), value.getDate());
    const distance = Math.round((day(date) - day(today)) / 86400000);
    const label = distance === 0 ? 'Today' : distance === 1 ? 'Tomorrow' : date.toLocaleDateString(locale, { month: 'short', day: 'numeric', ...(date.getFullYear() !== today.getFullYear() ? { year: 'numeric' } : {}) });
    return label + ', ' + date.toLocaleTimeString(locale, { hour: 'numeric', minute: '2-digit' });
  }
  const api = { mentioned, whoAnswers, answerHint, reduceStatus, statusLabel, route, inline, markdown, messagePart, marbleStatus, hues, nextHue, characterCount, minuteTime, timeMinute, recentTasks, deliveryLines, scheduleLabel, relativeRun };
  if (typeof module !== 'undefined') module.exports = api;
  else root.MarbleLogic = api;
})(typeof window === 'undefined' ? globalThis : window);
