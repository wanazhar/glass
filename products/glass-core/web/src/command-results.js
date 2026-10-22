// Human-facing command summaries; diagnostic data stays available on demand.
const node = (tag, cls, text) => {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== undefined) n.textContent = text;
  return n;
};
const label = key => String(key).replaceAll('_', ' ').replace(/\b\w/g, c => c.toUpperCase()).replace(/\bUsd\b/g, 'USD');
const titles = { fs_scan: 'File search cache', env: 'Environment', extensionIsolation: 'Extension isolation', expected: 'Revision', cost_usd: 'Reported cost (USD)', cwd: 'Working directory', args: 'Arguments', stdout: 'Output', stderr: 'Errors', id: 'ID', url: 'URL', api: 'API', input: 'Input', output: 'Output' };
const pretty = key => titles[key] || label(key).replace(/([a-z])([A-Z])/g, '$1 $2');
const display = value => value == null ? 'Not reported' : typeof value === 'boolean' ? (value ? 'Yes' : 'No') : String(value) || 'None';
export function renderCommandData(root, value, { command = '', markdown, privateEntry = () => false, onSelect } = {}) {
  let count = 0;
  function field(key, value) {
    const row = node('div', 'command-stat');
    row.append(node('dt', '', pretty(key)));
    const content = node('dd');
    if (typeof value === 'string' && ['proposal', 'draft', 'answer', 'text', 'diff', 'preview'].includes(key) && markdown) {
      content.className = 'prose';
      markdown(content, ['diff', 'preview'].includes(key) ? '```diff\n' + value.slice(0, 65536) + '\n```' : value.slice(0, 65536));
    } else if (typeof value === 'string' && (value.includes('\n') || ['output', 'stdout', 'stderr', 'patch', 'system', 'instructions'].includes(key))) {
      row.classList.add('command-stat-wide');
      const text = node('pre', 'command-literal', value.slice(0, 65536));
      content.append(text);

    } else if (['status', 'state', 'outcome'].includes(key) && typeof value === 'string') {
      content.append(node('span', 'command-badge', value.replaceAll('_', ' ')));
    } else content.textContent = display(value).slice(0, 65536);
    if (typeof value === 'string' && value.length > 65536) content.append(node('p', 'quiet', 'Preview limited to 64K characters. Narrow the query for more.'));
    if (['proposal', 'draft', 'answer', 'text', 'diff', 'preview'].includes(key)) row.classList.add('command-stat-wide');
    row.append(content);
    return row;
  }
  function group(data, title = '', depth = 0) {
    const section = node('section', 'command-group');
    if (privateEntry(data)) return section;
    if (++count > 3000 || depth > 16) { section.append(node('p', 'quiet', 'Display limit reached. Narrow the query.')); return section; }
    if (title) section.append(node('h3', '', pretty(title)));
    if (Array.isArray(data)) {
      if (!data.length) { section.append(node('p', 'command-empty', title ? `No ${pretty(title).toLowerCase()} to show.` : 'Nothing to show yet.')); return section; }
      const list = node('div', 'command-result-list');
      const cards = [];
      for (const item of data.slice(0, 300)) {
        if (++count > 3000) { list.append(node('p', 'quiet', 'Display limit reached. Narrow the query.')); break; }
        if (privateEntry(item)) continue;
        const card = node('article', 'command-list-item');
        if (item && typeof item === 'object') {
          const identity = ['name', 'title', 'label', 'command', 'path', 'model', 'session', 'id', 'record'].find(key => typeof item[key] === 'string' || typeof item[key] === 'number');
          if (identity) card.append(node('h4', '', ['id', 'record'].includes(identity) ? `${pretty(identity)} ${item[identity]}` : String(item[identity])));
          const rest = Object.fromEntries(Object.entries(item).filter(([key]) => key !== identity));
          card.append(group(rest, '', depth + 1));
          if (command === 'quote' && Number.isInteger(item.record) && onSelect) {
            const choose = node('button', 'quiet', 'Use this excerpt'); choose.type = 'button';
            choose.onclick = () => onSelect(item); card.append(choose);
          }
        } else {
          const text = display(item);
          card.classList.toggle('command-short-item', text.length < 80 && !text.includes('\n') && command !== 'prompts');
          card.append(node('p', '', text.slice(0, 65536)));
          if (text.length > 65536) card.append(node('p', 'quiet', 'Preview limited to 64K characters. Narrow the query for more.'));
        }
        list.append(card); cards.push(card);
        if (count >= 3000) break;
      }
      if (cards.length > 8) {
        const search = node('input', 'command-filter'); search.type = 'search';
        search.placeholder = `Filter ${title ? pretty(title).toLowerCase() : 'results'}…`;
        search.setAttribute('aria-label', search.placeholder.replace('…', ''));
        const status = node('p', 'command-list-count'); status.setAttribute('role', 'status');
        search.oninput = () => {
          const query = search.value.trim().toLowerCase(); let visible = 0;
          for (const card of cards) { card.hidden = !card.textContent.toLowerCase().includes(query); if (!card.hidden) visible++; }
          status.textContent = `${visible} of ${cards.length} ${cards.length === 1 ? 'item' : 'items'}`;
        };
        search.oninput(); section.append(search, status);
      }
      section.append(list);
      if (data.length > 300) section.append(node('p', 'quiet', 'Showing the first 300 items. Narrow the command query for more.'));
    } else if (data && typeof data === 'object') {
      const fields = node('dl', 'command-fields');
      for (const [key, item] of Object.entries(data)) {
        if (++count > 3000) { fields.append(node('p', 'quiet', 'Display limit reached. Narrow the query.')); break; }
        if (item && typeof item === 'object') continue;
        fields.append(field(key, item));
      }
      if (fields.children.length) section.append(fields);
      for (const [key, item] of Object.entries(data)) {
        if (count >= 3000) break;
        if (item && typeof item === 'object') section.append(group(item, key, depth + 1));
      }
    } else {
      section.append(node('p', 'command-message', display(data).slice(0, 65536)));
      if (display(data).length > 65536) section.append(node('p', 'quiet', 'Preview limited to 64K characters. Narrow the query for more.'));
    }
    return section;
  }
  function diagnostics(data) {
    const details = node('details', 'command-diagnostics');
    details.append(node('summary', '', 'More details'), group(data)); root.append(details);
  }
  if (command === 'guidance' && Array.isArray(value?.commands)) {
    root.append(node('p', 'command-intro', 'Find a command and see what it does.'));
    const search = node('input', 'command-filter'); search.type = 'search'; search.placeholder = 'Search commands…'; search.setAttribute('aria-label', 'Search guidance commands');
    const status = node('p', 'quiet'); status.setAttribute('role', 'status');
    const list = node('div', 'command-catalogue');
    for (const item of [...value.commands, ...(value.human_actions || [])].slice(0, 600)) {
      const card = node('article', 'command-list-item');
      card.append(node('h3', '', item.command), node('p', '', item.purpose || ''));
      list.append(card);
    }
    search.oninput = () => {
      const q = search.value.trim().toLowerCase(); let shown = 0;
      for (const card of list.children) { card.hidden = !card.textContent.toLowerCase().includes(q); if (!card.hidden) shown++; }
      status.textContent = `${shown} ${shown === 1 ? 'command' : 'commands'}`;
    };
    search.oninput(); root.append(search, status, list);
    if (value.topics?.length) root.append(group(value.topics, 'Workflow topics'));
    if (value.discovery) root.append(node('p', 'command-note', value.discovery));
    return;
  }
  if (command === 'usage' && value && typeof value === 'object') {
    const metrics = node('div', 'command-metrics');
    for (const [title, amount] of [['Requests', value.requests], ['Input tokens', value.reported_prompt_total], ['Output tokens', value.reported_output], ['Cache hit rate', value.reported_cache_hit_ratio == null ? null : `${(value.reported_cache_hit_ratio * 100).toFixed(1)}%`], ['Known cost', value.known_cost_subtotal_usd == null ? null : `$${value.known_cost_subtotal_usd.toFixed(4)}`]]) {
      const card = node('div', 'command-metric'); card.append(node('span', 'command-metric-label', title), node('strong', '', typeof amount === 'number' ? amount.toLocaleString() : display(amount))); metrics.append(card);
    }
    root.append(metrics);
    if (value.scope) root.append(node('p', 'command-note', value.scope));
    if (value.cost_complete === false) root.append(node('p', 'command-note', 'Cost is incomplete: some requests have no reported price.'));
    const coverage = {};
    for (const key of ['requests_with_usage', 'requests_without_usage', 'non_completed_requests']) if (key in value) coverage[key] = value[key];
    root.append(group(coverage, 'Reporting coverage'));
    if (value.cache_note) root.append(node('p', 'command-note', value.cache_note));
    diagnostics(value); return;
  }
  if ((command === 'status' || command === 'settings-info') && value && typeof value === 'object') {
    const main = {}, rest = { ...value };
    for (const key of ['connection', 'model', 'provider', 'session']) if (key in rest) { main[key] = rest[key]; delete rest[key]; }
    if (Object.keys(main).length) root.append(group(main, 'Connection'));
    root.append(group(rest)); return;
  }
  root.append(group(value));
}
