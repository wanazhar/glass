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
  const api = { mentioned, whoAnswers, answerHint, reduceStatus, statusLabel, route, inline, markdown, marbleStatus };
  if (typeof module !== 'undefined') module.exports = api;
  else root.MarbleLogic = api;
})(typeof window === 'undefined' ? globalThis : window);
