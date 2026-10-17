// Human-facing command summaries; diagnostic data stays available on demand.
const node = (tag, cls, text) => {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== undefined) n.textContent = text;
  return n;
};
const label = key => String(key).replaceAll('_', ' ').replace(/\b\w/g, c => c.toUpperCase()).replace(/\bUsd\b/g, 'USD');
const display = value => value == null ? 'Not reported' : typeof value === 'boolean' ? (value ? 'Yes' : 'No') : String(value) || 'None';
export function renderCommandData(root, value, { command = '', markdown, privateEntry = () => false } = {}) {
  let count = 0;
  function field(key, value) {
    const row = node('div', 'command-field');
    row.append(node('dt', '', label(key)));
    const content = node('dd');
    if (typeof value === 'string' && ['proposal', 'draft', 'answer', 'text', 'diff', 'preview'].includes(key) && markdown) {
      content.className = 'prose';
      markdown(content, ['diff', 'preview'].includes(key) ? '```diff\n' + value + '\n```' : value);
    } else content.textContent = display(value);
    row.append(content);
    return row;
  }
  function group(data, title = '', depth = 0) {
    const section = node('section', 'command-group');
    if (privateEntry(data)) return section;
    if (++count > 3000 || depth > 16) { section.append(node('p', 'quiet', 'Display limit reached. Narrow the query.')); return section; }
    if (title) section.append(node('h3', '', label(title)));
    if (Array.isArray(data)) {
      if (!data.length) section.append(node('p', 'quiet', 'None'));
      for (const item of data.slice(0, 300)) {
        if (++count > 3000) { section.append(node('p', 'quiet', 'Display limit reached. Narrow the query.')); break; }
        if (privateEntry(item)) continue;
        if (item && typeof item === 'object') {
          const name = item.command ?? item.name ?? item.title ?? item.id;
          const card = node('article', 'command-list-item');
          if (name != null) card.append(node('h4', '', String(name)));
          const rest = Object.fromEntries(Object.entries(item).filter(([key]) => !['command', 'name', 'title', 'id'].includes(key)));
          card.append(group(rest, '', depth + 1)); section.append(card);
        } else section.append(node('p', 'command-list-text', display(item)));
        if (count >= 3000) break;
      }
      if (data.length > 300) section.append(node('p', 'quiet', 'Showing the first 300 items. Narrow the query.'));
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
    } else section.append(node('p', '', display(data)));
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
      const card = node('div', 'command-metric'); card.append(node('span', 'quiet', title), node('strong', '', typeof amount === 'number' ? amount.toLocaleString() : display(amount))); metrics.append(card);
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
