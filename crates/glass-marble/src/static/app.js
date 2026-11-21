/* global MarbleLogic */
(() => {
    'use strict';
    const L = MarbleLogic;
    const t = text => text;
    const $ = id => document.getElementById(id);
    const enc = encodeURIComponent;
    const s = {
        data: { tasks: [], conversations: [], approvals: [], memory: {}, routines: {}, control: {} },
        people: [], shared: [], entries: [], status: { stream: false, requestFailed: false, engine: false },
        health: {}, route: L.route(location.hash), generation: 0, refreshId: 0, sequence: 0,
        defaultChat: undefined, missing: false, transcriptError: '', live: new Map(), drafts: new Map(),
        errors: new Map(), open: new Set(), loaded: false, busy: false, tools: null, crates: null, approvalChats: new Map(),
    };
    const events = ['hello', 'turn_start', 'waiting', 'text', 'reasoning', 'tool', 'tool_start', 'tool_end', 'tool_progress',
        'tool_cancel', 'usage', 'compacted', 'steered', 'queued', 'notice', 'error', 'settled', 'cancel_requested',
        'approval', 'approval_pending', 'approval_resolved', 'state', 'control_denied', 'control', 'memory',
        'memory_suggestion', 'routine', 'retention', 'steer_queued', 'steer_rejected', 'conversation', 'prism', 'everyone', 'handoff', 'runtime'];
    function el(tag, cls, ...children) {
        const node = document.createElement(tag);
        if (cls) node.className = cls;
        node.append(...children.filter(x => x !== null && x !== undefined));
        return node;
    }
    function button(label, action, cls = '') {
        const b = el('button', cls, label); b.type = 'button'; b.addEventListener('click', action); return b;
    }
    function link(label, href, cls = '') { const a = el('a', cls, label); a.href = href; return a; }
    function svgNode(tag, attrs) {
        const node = document.createElementNS('http://www.w3.org/2000/svg', tag);
        for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
        return node;
    }
    function icon(name) {
        const paths = { everyone: 'M3 16v-2a4 4 0 0 1 4-4h3a4 4 0 0 1 4 4v2M15 4a3 3 0 0 1 0 6m1 1a4 4 0 0 1 3 4M11 5a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z', marbles: 'M8 6a4 4 0 1 1-8 0 4 4 0 0 1 8 0Zm12 0a4 4 0 1 1-8 0 4 4 0 0 1 8 0ZM8 16a4 4 0 1 1-8 0 4 4 0 0 1 8 0Zm12 0a4 4 0 1 1-8 0 4 4 0 0 1 8 0Z', activity: 'M1 11h4l3-8 5 15 3-7h5', settings: 'M4 5h16M4 11h16M4 17h16M8 2v6m8 0v6m-6 0v6', arrow: 'm8 4-6 6 6 6m-6-6h17', chevron: 'm8 5 5 5-5 5', tool: 'm5 2 5 5-3 3-5-5v5l4 4 4-1 7 7 3-3-7-7 1-4-4-4Z', more: 'M4 10h1m5 0h1m5 0h1' };
        paths.close = 'M5 5l12 12M17 5 5 17';
        const svg = svgNode('svg', { viewBox: '0 0 22 22', class: 'icon', 'aria-hidden': 'true' });
        svg.append(svgNode('path', { d: paths[name] || paths.tool })); return svg;
    }
    function glyph(person = {}) {
        const svg = svgNode('svg', { viewBox: '0 0 28 28', class: 'glyph', 'aria-hidden': 'true' });
        const shapes = { circle: ['circle', { cx: 14, cy: 14, r: 12 }], hex: ['polygon', { points: '7,2 21,2 28,14 21,26 7,26 0,14' }], shield: ['path', { d: 'M2 3h24v10c0 8-12 13-12 13S2 21 2 13Z' }], drop: ['path', { d: 'M14 1C11 7 3 12 3 18a11 9 0 0 0 22 0C25 12 17 7 14 1Z' }], star: ['polygon', { points: '14,1 18,9 27,11 21,18 22,27 14,23 6,27 7,18 1,11 10,9' }], square: ['rect', { x: 2, y: 2, width: 24, height: 24, rx: 6 }] };
        const [tag, attrs] = shapes[person.glyph] || shapes.circle;
        const hue = Number.isFinite(Number(person.hue)) ? Number(person.hue) : 210;
        svg.append(svgNode(tag, { ...attrs, fill: `hsl(${hue} 38% 69%)` }));
        for (const cx of [10, 18]) svg.append(svgNode('circle', { cx, cy: 14, r: 1.35, fill: '#252a33' }));
        return svg;
    }
    const person = id => s.people.find(p => p.id === id);
    const personality = p => p.role && !p.isDefault ? p.role : p.persona?.split('\n')[0] || t('Your everyday Marble');
    const current = () => person(s.route.id || 'marble');
    const chatRoute = id => '#/m/' + enc(id);
    const tasks = () => s.data.tasks || [];
    const approvals = () => s.data.approvals || [];
    const suggestions = () => s.data.memory?.suggestions || [];
    const personForChat = id => s.people.find(p => !p.isDefault && p.conversationId === id) || person('marble');
    function defaultChats() {
        const owned = new Set(s.people.filter(p => !p.isDefault).map(p => p.conversationId));
        return (s.data.conversations || []).filter(c => c.id !== 'everyone' && !owned.has(c.id)).sort((a, b) => b.updatedAt - a.updatedAt);
    }
    function conversation() { return current()?.isDefault ? s.defaultChat : current()?.conversationId; }
    function running() { const id = conversation(); return tasks().find(task => id && task.conversationId === id && task.status === 'running'); }
    function humanError(error) {
        return String(error.message || error).replace(/\bprism\b/ig, t('Marble')).replace(/\bdispatch\b/ig, t('send')).replace(/\bswarm\b/ig, t('group')).replace(/\blane\b/ig, t('chat')).replace(/\broster\b/ig, t('Marbles'));
    }
    async function api(path, body, options = {}) {
        let response;
        try { response = await fetch(path, { credentials: 'same-origin', ...options, ...(body === undefined ? {} : { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) }) }); }
        catch (error) { s.status = L.reduceStatus(s.status, { type: 'request', ok: false }); renderStatus(); throw error; }
        s.status = L.reduceStatus(s.status, { type: 'request', ok: true }); renderStatus();
        if (response.status === 401) {
            if (!$('unlock').open) $('unlock').showModal();
            throw Object.assign(new Error(t('Unlock this host to continue.')), { status: 401 });
        }
        const value = await response.json();
        if (path === '/healthz') { s.health = value; s.status = L.reduceStatus(s.status, { type: 'health', health: value }); renderStatus(); return value; }
        if (!response.ok) throw Object.assign(new Error(value.message || value.error || t('Request failed')), { status: response.status });
        return value;
    }
    async function action(key, path, body = {}, after) {
        if (document.activeElement.tagName === 'BUTTON') document.activeElement.blur();
        s.errors.delete(key); showError(key, '');
        try { const result = await api(path, body); if (after) after(result); await refresh(); return true; }
        catch (error) { s.errors.set(key, humanError(error)); showError(key, humanError(error)); return false; }
    }
    function errorNode(key) { const n = el('p', 'error', s.errors.get(key) || ''); n.dataset.error = key; n.setAttribute('role', 'alert'); return n; }
    function showError(key, message) { document.querySelectorAll('[data-error]').forEach(n => { if (n.dataset.error === key) n.textContent = message; }); }
    function statusText(p) {
        const relevant = tasks().filter(task => p.isDefault ? !s.people.some(m => !m.isDefault && m.conversationId === task.conversationId) : task.conversationId === p.conversationId);
        const waiting = approvals().map(approval => ({ ...approval, owner: approvalOwner(approval)?.id }));
        return L.marbleStatus(p, relevant, waiting);
    }
    function renderStatus() { document.querySelectorAll('[data-connection]').forEach(n => { n.textContent = L.statusLabel(s.status); n.parentNode.dataset.connected = String(L.statusLabel(s.status) === 'Connected'); }); }
    function connection(cls = '') {
        const text = el('span'); text.dataset.connection = ''; text.textContent = L.statusLabel(s.status);
        const n = el('div', 'connection ' + cls, el('span', 'dot'), text); n.dataset.connected = String(L.statusLabel(s.status) === 'Connected'); n.setAttribute('role', 'status'); return n;
    }
    function nav(label, screen, count) {
        const active = screen === 'marbles' ? ['marbles', 'chat', 'info', 'new'].includes(s.route.screen) : screen === s.route.screen;
        const a = link(null, '#/' + screen, 'nav-item' + (active ? ' active' : ''));
        if (active) a.setAttribute('aria-current', 'page');
        a.append(icon(screen), el('span', '', label)); if (count) a.append(el('span', 'badge', String(count))); return a;
    }
    function renderRail() {
        const count = approvals().length + suggestions().length;
        $('rail').replaceChildren(el('div', 'brand', glyph(), t('Marble')), nav(t('Everyone'), 'everyone'));
        $('rail').append(el('div', 'rail-heading', el('span', '', t('Marbles')), link(t('New Marble'), '#/new', 'button')));
        for (const p of s.people) {
            const a = link(null, chatRoute(p.id), 'nav-item' + (s.route.id === p.id ? ' active' : ''));
            a.append(glyph(p), el('div', 'row-copy', el('strong', '', p.name), el('span', '', personality(p)), el('span', '', statusText(p))));
            if (s.route.id === p.id) a.setAttribute('aria-current', 'page'); $('rail').append(a);
        }
        $('rail').append(el('div', 'rail-bottom', nav(t('Activity'), 'activity', count), nav(t('Settings'), 'settings'), connection()));
        $('tabs').replaceChildren(nav(t('Everyone'), 'everyone'), nav(t('Marbles'), 'marbles'), nav(t('Activity'), 'activity', count), nav(t('Settings'), 'settings'));
    }
    function overflow() {
        const details = el('details', 'menu'); const summary = el('summary', '', icon('more')); summary.setAttribute('aria-label', t('Everyone options'));
        details.append(summary, el('div', 'menu-content', button(t('Clear'), () => {
            details.open = false;
            if (confirm(t('Clear Everyone? This deletes the shared timeline, not the individual chats.'))) action('conversation', '/api/everyone/clear');
        }, 'danger'))); return details;
    }
    function earlierChats() {
        const details = el('details', 'menu', el('summary', '', t('Earlier chats')));
        const items = el('div', 'menu-content');
        const search = el('input'); search.type = 'search'; search.placeholder = t('Search chats'); search.setAttribute('aria-label', t('Search chats'));
        const results = el('div'); let searchId = 0;
        const choose = id => { s.defaultChat = id; s.missing = false; s.transcriptError = ''; s.entries = []; s.generation++; details.open = false; render(); refresh(); };
        const rows = (chats) => { results.replaceChildren(); for (const chat of chats) {
            const row = el('div', 'record', button(chat.title || t('Untitled chat'), () => choose(chat.id), 'quiet'));
            row.append(el('div', 'actions', button(t('Rename'), () => { const title = prompt(t('Chat name'), chat.title); if (title?.trim()) action('conversation', '/api/conversations/' + enc(chat.id) + '/rename', { title: title.trim() }); }), button(t('Delete'), () => {
                if (confirm(t('Delete this chat and its history?'))) action('conversation', '/api/conversations/' + enc(chat.id) + '/delete', {}, () => { if (s.defaultChat === chat.id) choose(null); });
            }, 'danger'))); results.append(row);
        } };
        rows(defaultChats());
        search.addEventListener('input', async () => {
            const request = ++searchId;
            try { const result = await api('/api/transcript/search?q=' + enc(search.value)); if (request !== searchId) return;
                const ids = new Set((result.hits || []).map(e => e.conversationId));
                rows(defaultChats().filter(c => c.title.toLowerCase().includes(search.value.toLowerCase()) || ids.has(c.id)));
            } catch (error) { if (request === searchId) results.replaceChildren(el('p', 'error', humanError(error))); }
        });
        items.append(search, results, link(t('Export this chat'), '/api/export?conversationId=' + enc(conversation() || ''), 'button'));
        details.append(items); return details;
    }
    function header() {
        const screen = s.route.screen; const p = current();
        const chat = screen === 'chat' || screen === 'info';
        const title = chat ? p?.name || t('Missing Marble') : ({ everyone: t('Everyone'), marbles: t('Marbles'), activity: t('Activity'), settings: t('Settings'), new: t('New Marble') }[screen] || t('Not found'));
        const titleBlock = el('div', 'heading', el('h1', '', title));
        if (chat && p) titleBlock.append(el('p', 'muted', statusText(p)));
        if (screen !== 'everyone') titleBlock.append(connection('mobile-status'));
        const actions = el('div', 'header-actions');
        if (screen === 'everyone') actions.append(connection('mobile-status'), overflow());
        if (chat && p) {
            if (p.isDefault) actions.append(earlierChats(), button(t('New chat'), () => { s.defaultChat = null; s.generation++; s.entries = []; s.missing = false; s.transcriptError = ''; s.errors.delete('conversation'); render(); }));
            const infoToggle = link(t('Info'), chatRoute(p.id) + (screen === 'info' ? '' : '/info'), 'button');
            infoToggle.setAttribute('aria-expanded', String(screen === 'info')); actions.append(infoToggle);
        }
        if (screen === 'marbles') actions.append(link(t('New Marble'), '#/new', 'button'));
        $('topbar').replaceChildren();
        if (chat || screen === 'new') {
            const back = link(icon('arrow'), screen === 'info' ? chatRoute(p?.id || 'marble') : '#/marbles', 'button quiet back'); back.setAttribute('aria-label', t('Back')); $('topbar').append(back);
        }
        if (chat && p) $('topbar').append(glyph(p));
        $('topbar').append(titleBlock, actions);
    }
    function prose(text, fallback) {
        const container = el('div', 'prose');
        const appendInline = (parent, content) => content.forEach(part => parent.append(part.tag === 'text' ? document.createTextNode(part.text) : el(part.tag, '', part.text)));
        for (const block of L.markdown(L.messagePart(text, fallback))) {
            const node = el(block.tag);
            if (block.tag === 'pre') node.append(el('code', '', block.text));
            else if (block.items) for (const item of block.items) { const li = el('li'); appendInline(li, item); node.append(li); }
            else appendInline(node, block.content);
            container.append(node);
        }
        return container;
    }
    function disclosure(label, child, key) {
        const d = el('details', 'thought', el('summary', '', label), child); d.open = s.open.has(key);
        d.addEventListener('toggle', () => d.open ? s.open.add(key) : s.open.delete(key)); return d;
    }
    function toolStep(tool) {
        const name = tool.tool || tool.name || t('Tool');
        const label = name === 'bash' ? t('Running command') : t('Using ') + name;
        return el('div', 'tool-step', icon('tool'), el('span', '', label, tool.text || tool.summary ? ' · ' + String(tool.text || tool.summary).slice(0, 240) : ''));
    }
    function entryNode(entry, fallback, key) {
        if (entry.kind === 'handoff') return el('p', 'handoff', L.handoffLine(entry));
        if (entry.kind === 'system') return el('div', 'system-line', entry.text);
        if (entry.kind === 'reasoning') return disclosure(t('Thought'), prose(entry.text), key);
        if (entry.kind === 'tool') return toolStep({ tool: entry.text });
        const own = entry.kind === 'user';
        const p = person(entry.speaker) || fallback || person('marble');
        const body = el('div', 'message-body');
        if (!own) {
            const name = el('span', '', entry.speakerName || p?.name || t('Marble'));
            name.style.color = `hsl(${Number(p?.hue || 210)} 38% 52%)`;
            const time = el('time', '', entry.at ? new Date(entry.at).toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }) : '');
            body.append(el('div', 'speaker', name, time));
        }
        body.append(own ? prose(entry.text) : prose(entry, entry.text));
        if (!own && s.data.voice?.enabled && entry.text) body.append(button(t('Read aloud'), () => readAloud(entry.text), 'quiet small'));
        return own ? el('article', 'message user', body) : el('article', 'message', glyph(p), body);
    }
    function retry(task) { return button(t('Retry'), () => action(s.route.screen === 'activity' ? 'activity' : 'conversation', '/api/turns/retry', { taskId: task.id }, result => { if (current()?.isDefault && result.conversationId) s.defaultChat = result.conversationId; })); }
    function readAloud(text) {
        if (!('speechSynthesis' in window)) { s.errors.set('conversation', t('Read aloud is not available in this browser.')); showError('conversation', s.errors.get('conversation')); return; }
        const utterance = new SpeechSynthesisUtterance(text); utterance.rate = Math.min(1, Math.max(0.1, Number(s.data.voice?.rate) || 0.5));
        utterance.voice = speechSynthesis.getVoices().find(voice => voice.name === s.data.voice?.voice) || null;
        speechSynthesis.cancel(); speechSynthesis.speak(utterance);
    }
    function approvalOwner(approval) {
        const chat = s.approvalChats.get(approval.requestId); if (chat) return personForChat(chat);
        for (const live of s.live.values()) if (live.tools.some(tool => tool.callId === approval.callId)) return personForChat(live.conversationId);
        const busy = tasks().filter(task => task.status === 'running');
        return busy.length === 1 ? personForChat(busy[0].conversationId) : null;
    }
    function approvalCard(approval) {
        const owner = approvalOwner(approval);
        const args = approval.arguments || {};
        const effect = /write|edit|apply|delete|mkdir|create/.test(approval.tool) ? t('change') : /shell|bash|exec/.test(approval.tool) ? t('run') : t('use');
        const target = args.path || args.command || args.url || approval.tool;
        const key = 'approval-' + approval.requestId;
        const card = el('div', 'request-card', el('h3', '', (owner?.name || t('A Marble')) + t(' wants to ') + effect + ' ' + target));
        card.append(el('p', 'muted small', approval.tool), prose(JSON.stringify(args, null, 2)));
        if (approval.conflict) card.append(el('p', 'muted', approval.conflict));
        card.append(el('div', 'actions', ...[[t('Allow'), 'allow', false], [t('Always allow here'), 'allow', true], [t('Deny'), 'deny', false]].map(([label, answer, remember]) => button(label, () => action(key, '/api/approvals/' + enc(approval.requestId) + '/' + answer, { remember })))), errorNode(key));
        return card;
    }
    function suggestionCard(item) {
        const owner = personForChat(item.conversationId); const key = 'suggestion-' + item.id;
        return el('div', 'request-card', el('h3', '', t('Save to ') + (owner?.name || t('Marble')) + t("'s memory?")), prose(item.text),
            el('div', 'actions', button(t('Save'), () => action(key, '/api/memory/' + enc(item.id) + '/accept')), button(t('Dismiss'), () => action(key, '/api/memory/' + enc(item.id) + '/dismiss'))), errorNode(key));
    }
    function requestCards(all = false) {
        const id = conversation(); const fragment = document.createDocumentFragment();
        for (const approval of approvals()) {
            const owner = approvalOwner(approval);
            if (all || (owner && owner.id === current()?.id) || (!owner && !!running())) fragment.append(approvalCard(approval));
        }
        for (const item of suggestions()) if (all || item.conversationId === id) fragment.append(suggestionCard(item));
        return fragment;
    }
    function renderTimeline(shared = false) {
        const previous = document.documentElement.scrollHeight - window.scrollY - window.innerHeight;
        const timeline = el('div', 'timeline');
        if (!shared && (s.missing || s.transcriptError)) {
            const n = el('div', 'empty', el('h2', '', s.missing ? t('This chat no longer exists') : t('Could not load this chat')), el('p', '', s.transcriptError), el('div', 'actions', link(t('Back to Marbles'), '#/marbles', 'button')));
            if (!s.missing) n.append(button(t('Try again'), refresh)); $('main').replaceChildren(n); return;
        }
        const entries = shared ? s.shared : s.entries;
        if (entries.length) timeline.append(el('p', 'day', new Date(entries[0].at).toLocaleDateString([], { month: 'long', day: 'numeric' })));
        entries.forEach((entry, i) => {
            if (!shared && entry.kind === 'assistant') s.live.get(entry.taskId)?.tools.forEach(tool => timeline.append(toolStep(tool)));
            timeline.append(entryNode(entry, shared ? null : current(), `${entry.taskId}-${entry.kind}-${i}`));
        });
        if (!shared) {
            const settled = new Set(entries.filter(e => e.kind === 'assistant').map(e => e.taskId));
            for (const [id, live] of s.live) {
                if (live.conversationId !== conversation() || settled.has(id)) continue;
                if (live.reasoning) timeline.append(disclosure(t('Thought'), prose(live.reasoning), id + '-live'));
                live.tools.forEach(tool => timeline.append(toolStep(tool)));
                if (live.text) timeline.append(entryNode({ kind: 'assistant', text: live.text }, current(), id));
            }
            for (const task of tasks().filter(task => task.conversationId === conversation() && ['failed', 'cancelled', 'interrupted'].includes(task.status))) {
                timeline.append(el('div', 'system-line', task.status === 'cancelled' ? t('Cancelled') : task.status === 'interrupted' ? t('Interrupted') : t("Couldn't finish"), el('div', 'actions', retry(task))));
            }
            timeline.append(requestCards());
        }
        if (!entries.length && !timeline.children.length) {
            timeline.append(el('div', 'empty', shared ? icon('everyone') : glyph(current()), el('h2', '', shared ? t('Everyone') : t('Start a chat')), el('p', '', shared ? t('Your messages and the Marbles’ replies appear here.') : t('Send a message to ') + (current()?.name || t('Marble')) + '.')));
        }
        timeline.append(errorNode('conversation'));
        $('main').replaceChildren(timeline);
        if (previous < 140) requestAnimationFrame(() => window.scrollTo(0, document.documentElement.scrollHeight));
    }
    function renderPeople() {
        const page = el('div', 'page', el('div', 'people-list'));
        for (const p of s.people) {
            const row = link(null, chatRoute(p.id), 'person-row');
            row.append(glyph(p), el('div', 'row-copy', el('strong', '', p.name), el('span', 'muted', personality(p))), el('span', 'status-text', statusText(p)), icon('chevron')); page.firstChild.append(row);
        }
        $('main').replaceChildren(page);
    }
    function composer() {
        const shared = s.route.screen === 'everyone';
        if (!(shared || ['chat', 'info'].includes(s.route.screen)) || !current() && !shared || s.missing || s.transcriptError || s.route.screen === 'info' && innerWidth < 1200) { $('composer-area').replaceChildren(); return; }
        const key = shared ? 'everyone' : 'chat-' + s.route.id;
        if ($('composer-area').dataset.key === key && $('message')) { updateComposer(); return; }
        $('composer-area').dataset.key = key;
        const input = el('textarea'); input.id = 'message'; input.rows = 1; input.value = s.drafts.get(key) || '';
        input.setAttribute('aria-label', shared ? t('Message everyone') : t('Message ') + current().name);
        const hint = el('p', 'compose-hint'); hint.id = 'compose-hint';
        const mentions = el('div', 'mentions'); mentions.id = 'mentions'; mentions.hidden = true; mentions.setAttribute('role', 'listbox');
        let choices = []; let selected = 0; let match = null;
        const choose = p => {
            if (!match) return;
            const before = input.value.slice(0, input.selectionStart); const rest = input.value.slice(input.selectionStart);
            input.value = before.slice(0, match.index) + '@' + p.name + ' ' + rest;
            const cursor = match.index + p.name.length + 2; input.focus(); input.setSelectionRange(cursor, cursor); mentions.hidden = true; changed(false);
        };
        const drawChoices = () => {
            mentions.replaceChildren(); choices.forEach((p, i) => {
                const option = button(p.name, () => choose(p), 'quiet'); option.setAttribute('role', 'option'); option.setAttribute('aria-selected', String(i === selected)); option.id = 'mention-' + i; mentions.append(option);
            }); input.setAttribute('aria-expanded', String(!mentions.hidden)); if (choices.length) input.setAttribute('aria-activedescendant', 'mention-' + selected);
        };
        function changed(complete = true) {
            s.drafts.set(key, input.value); input.style.height = '36px'; input.style.height = Math.min(154, input.scrollHeight) + 'px'; input.style.overflowY = input.scrollHeight > 154 ? 'auto' : 'hidden'; updateComposer();
            if (!shared || !complete) return;
            match = /@[\p{L}\p{N}_-]*$/u.exec(input.value.slice(0, input.selectionStart));
            choices = match ? [...s.people.filter(p => !p.isDefault), { name: 'everyone' }].filter(p => p.name.toLowerCase().startsWith(match[0].slice(1).toLowerCase())) : [];
            selected = 0; mentions.hidden = !choices.length; drawChoices();
        }
        input.addEventListener('input', () => changed());
        input.addEventListener('keydown', event => {
            if (event.isComposing) return;
            if (!mentions.hidden && ['ArrowDown', 'ArrowUp', 'Enter', 'Tab', 'Escape'].includes(event.key)) {
                event.preventDefault(); if (event.key === 'Escape') mentions.hidden = true;
                else if (event.key === 'Enter' || event.key === 'Tab') choose(choices[selected]);
                else { selected = (selected + (event.key === 'ArrowDown' ? 1 : -1) + choices.length) % choices.length; drawChoices(); } return;
            }
            if (event.key === 'Enter' && !event.shiftKey) { event.preventDefault(); send(); }
        });
        const sendButton = button(t('Send'), () => { if (running() && !shared && !input.value.trim()) stop(running()); else send(); }, 'primary'); sendButton.id = 'send';
        $('composer-area').replaceChildren(el('div', 'compose-wrap', mentions, hint, el('div', 'composer', input, sendButton), errorNode('send')));
        if (shared) { input.setAttribute('role', 'combobox'); input.setAttribute('aria-controls', 'mentions'); input.setAttribute('aria-autocomplete', 'list'); }
        updateComposer();
    }
    function updateComposer() {
        if (!$('message')) return;
        const shared = s.route.screen === 'everyone'; const active = !shared && running();
        $('message').placeholder = shared ? t('Message everyone. Use @name to ask one Marble.') : active ? t('Add to the current reply…') : t('Message ') + (current()?.name || t('Marble')) + '…';
        $('compose-hint').textContent = shared && $('message').value.trim() ? L.answerHint($('message').value, s.people) : active ? t('Adds to the current reply.') : '';
        $('compose-hint').hidden = !$('compose-hint').textContent;
        $('send').textContent = active ? $('message').value.trim() ? t('Add') : t('Stop') : t('Send'); $('send').disabled = s.busy;
    }
    function stop(task) { if (task) action('conversation', '/api/tasks/' + enc(task.id) + '/cancel'); }
    async function send() {
        if (s.busy) return;
        const input = $('message'); const text = input?.value.trim(); if (!text) return;
        const shared = s.route.screen === 'everyone'; const active = !shared && running(); const p = current(); const generation = s.generation;
        const path = shared ? '/api/everyone' : active ? '/api/turns/steer' : '/api/turns';
        const body = shared ? { text } : active ? { text, taskId: active.id, conversationId: active.conversationId } : { text, ...(conversation() ? { conversationId: conversation() } : {}), ...(!p.isDefault ? { marbleId: p.id } : {}) };
        const newDefault = !shared && !active && p.isDefault && !conversation();
        const knownChats = new Set(defaultChats().map(chat => chat.id));
        s.busy = true; updateComposer();
        await action('send', path, body, result => {
            s.drafts.delete(shared ? 'everyone' : 'chat-' + p.id);
            if (generation !== s.generation) return;
            input.value = ''; input.style.height = '36px'; if (!shared && p.isDefault && result.conversationId) s.defaultChat = result.conversationId;
            if (newDefault && !result.conversationId) s.pendingDefault = { knownChats, text };
        });
        s.busy = false; updateComposer();
    }
    function section(title, ...nodes) { return el('section', '', el('h2', '', title), ...nodes); }
    function field(label, value = '', type = 'text') {
        const input = el(type === 'textarea' ? 'textarea' : 'input'); if (type !== 'textarea') input.type = type;
        if (type === 'checkbox') input.checked = !!value; else input.value = value ?? '';
        const wrapper = el('label', 'field' + (type === 'checkbox' ? ' check-field' : ''), el('span', '', label), input); return { node: wrapper, input };
    }
    function selectField(label, options, value) {
        const select = el('select'); for (const [key, title] of options) { const option = el('option', '', title); option.value = key; select.append(option); } select.value = value || options[0]?.[0];
        return { node: el('label', 'field', el('span', '', label), select), input: select };
    }
    function form(key, fields, save) {
        const f = el('form', '', ...fields.map(x => x.node), button(t('Save'), () => f.requestSubmit(), 'primary'), errorNode(key));
        f.dataset.key = key;
        const dirty = () => { f.dataset.dirty = 'true'; };
        f.addEventListener('input', dirty); f.addEventListener('change', dirty);
        f.addEventListener('click', event => { if (event.target.closest('.look-options')) dirty(); });
        f.addEventListener('submit', async event => {
            event.preventDefault(); document.activeElement.blur();
            const submit = f.querySelector('.primary'); submit.disabled = true;
            try { if (await save()) { delete f.dataset.dirty; render(); } } finally { submit.disabled = false; }
        }); return f;
    }
    function personalityField(value = '') {
        const item = field(t('Personality'), value, 'textarea');
        const counter = el('small', 'muted'); item.node.append(counter);
        const update = () => {
            const count = L.characterCount(item.input.value);
            counter.textContent = count + t(' / 2000');
            item.input.setCustomValidity(count > 2000 ? t('A personality is at most 2000 characters.') : '');
        };
        item.input.addEventListener('input', update); update(); return item;
    }
    function lookEditor(p) {
        const draft = { ...p }; const container = el('fieldset', 'look-editor');
        container.append(el('legend', '', t('Look')));
        const preview = el('div', 'look-preview'); const shapes = el('div', 'look-options'); const hues = el('div', 'look-options');
        shapes.setAttribute('aria-label', t('Shape')); hues.setAttribute('aria-label', t('Hue'));
        const redraw = () => {
            preview.replaceChildren(glyph(draft), el('span', '', draft.name || t('Your Marble')));
            shapes.querySelectorAll('button').forEach(b => b.setAttribute('aria-pressed', String(b.dataset.shape === draft.glyph)));
            hues.querySelectorAll('button').forEach(b => b.setAttribute('aria-pressed', String(Number(b.dataset.hue) === draft.hue)));
        };
        for (const shape of s.shapes || ['circle', 'hex', 'shield', 'drop', 'star', 'square']) {
            const b = button(glyph({ ...draft, glyph: shape }), () => { draft.glyph = shape; redraw(); });
            b.dataset.shape = shape; b.setAttribute('aria-label', shape); shapes.append(b);
        }
        for (const hue of [...new Set([...L.hues, Number(draft.hue)])]) {
            const b = button(glyph({ glyph: 'circle', hue }), () => { draft.hue = hue; redraw(); });
            b.dataset.hue = hue; b.setAttribute('aria-label', t('Hue ') + hue); hues.append(b);
        }
        container.append(preview, el('small', 'muted', t('Shape')), shapes, el('small', 'muted', t('Hue')), hues); redraw();
        return { node: container, draft, setName: name => { draft.name = name; redraw(); } };
    }
    function infoPage(p) {
        const key = 'info'; const page = el('div', 'page info-page'); if (!p) return page;
        const name = field(t('Name'), p.name); name.input.required = true; name.input.maxLength = 40;
        const role = field(t('Role'), p.role); const persona = personalityField(p.persona);
        const look = lookEditor(p); name.input.addEventListener('input', () => look.setName(name.input.value));
        const folder = field(t('Workspace folder'), p.workspace); folder.input.placeholder = t('Use the host workspace');
        const fields = p.isDefault ? [persona] : [name, look, role, persona, folder];
        const identity = form(key, fields, () => action(key, '/api/prism/marbles/' + enc(p.id) + '/update', p.isDefault ? { persona: persona.input.value } : { name: name.input.value.trim(), role: role.input.value, persona: persona.input.value, glyph: look.draft.glyph, hue: look.draft.hue, workspace: folder.input.value }));
        identity.classList.add('identity-form'); identity.dataset.form = 'identity';
        if (p.isDefault) page.append(el('div', 'look-preview', glyph(p), el('span', '', p.name)), el('p', 'muted small', t('The default Marble keeps its name and look.')));
        page.append(section(t('About'), identity));
        const memory = section(t('Memory')); const facts = (s.data.memory?.facts || []).filter(fact => !fact.owner || fact.owner === p.id);
        if (!facts.length) memory.append(el('p', 'muted small', t('No memories yet. Add something worth keeping.')));
        for (const fact of facts) {
            const record = el('div', 'record', el('small', 'muted', fact.owner ? t('Only ') + p.name : t('Shared')), prose(fact.text));
            record.dataset.fact = fact.id;
            record.append(el('div', 'actions', button(t('Edit'), () => {
                const text = field(t('Memory'), fact.text, 'textarea');
                const editor = form(key, [text], () => action(key, '/api/memory/' + enc(fact.id) + '/update', { text: text.input.value, kind: fact.kind }));
                editor.append(button(t('Cancel'), () => { delete editor.dataset.dirty; document.activeElement.blur(); render(); }));
                record.replaceChildren(el('small', 'muted', fact.owner ? t('Only ') + p.name : t('Shared')), editor); text.input.focus();
            }), button(t('Forget'), () => action(key, '/api/memory/' + enc(fact.id) + '/forget')))); memory.append(record);
        }
        const fact = field(t('Add a fact'), '', 'textarea'); fact.input.required = true;
        const shared = field(t('Shared with all Marbles'), false, 'checkbox');
        const memoryForm = form('memory-add', [fact, shared], () => action('memory-add', '/api/memory', { text: fact.input.value.trim(), owner: shared.input.checked ? '' : p.id }));
        memoryForm.classList.add('memory-add'); memoryForm.dataset.form = 'memory'; memory.append(memoryForm); page.append(memory);
        const capabilities = section(t('What ') + p.name + t(' can do'), el('p', 'small', t('Workspace: ') + (p.workspace || t('Host workspace'))), el('p', 'small', t('Model: ') + (s.data.model || t('Engine default'))), el('p', 'muted small', t('Per-Marble models are coming')));
        if (s.tools) {
            for (const [title, items] of [[t('Coding tools'), s.tools.core?.observed || []], [t('Workspace tools'), s.tools.governed || []]]) {
                const group = el('details', '', el('summary', '', title + ' · ' + items.length)); const list = el('ul', 'tools-list');
                if (!items.length) list.append(el('li', '', t('No tools observed yet.')));
                for (const tool of items) list.append(el('li', '', tool.description || tool.name));
                group.append(list); capabilities.append(group);
            }
        } else capabilities.append(el('p', 'muted small', s.toolsError || t('Loading tools…')), button(t('Try again'), () => { s.toolsError = ''; refresh(); }));
        page.append(capabilities);
        if (!p.isDefault) page.append(section(t('Remove Marble'), el('p', 'muted small', t('Removing a Marble deletes its chat too.')), button(t('Remove ') + p.name, () => {
            if (confirm(t('Remove ') + p.name + t('? Its chat is deleted too.'))) action('remove', '/api/prism/marbles/' + enc(p.id) + '/remove', {}, () => { location.hash = '#/marbles'; });
        }, 'danger'), errorNode('remove')));
        return page;
    }
    function newPage() {
        const key = 'new'; const page = el('div', 'page new-page');
        const name = field(t('Name')); name.input.required = true; name.input.maxLength = 40; name.input.placeholder = t('For example, Ada');
        const persona = personalityField(); persona.input.placeholder = t('What should this Marble be like?');
        const look = lookEditor({ name: '', glyph: s.shapes?.[0] || 'circle', hue: L.nextHue(s.people) });
        name.input.addEventListener('input', () => look.setName(name.input.value));
        const workspace = field(t('Workspace folder (optional)')); workspace.input.placeholder = t('Use the host workspace');
        const count = s.people.filter(p => !p.isDefault).length;
        if (count >= Math.max(0, (s.limit || 8) - 2)) page.append(el('p', 'muted small', count + t(' of ') + (s.limit || 8) + t(' Marbles')));
        const create = form(key, [name, persona, look, workspace], () => action(key, '/api/prism/marbles', { name: name.input.value.trim(), persona: persona.input.value, glyph: look.draft.glyph, hue: look.draft.hue, workspace: workspace.input.value }, result => { location.hash = chatRoute(result.marble.id) + '/info'; }));
        create.classList.add('identity-form'); create.dataset.form = 'create'; create.querySelector('.primary').textContent = t('Create Marble');
        page.append(create); return page;
    }
    function activityPage() {
        const key = 'activity'; const page = el('div', 'page activity-page', errorNode(key));
        if (s.data.control.paused) page.append(el('div', 'paused-banner', el('p', '', t('New messages are paused. Resume when you are ready.')), button(t('Resume'), () => action(key, '/api/control/resume'))));
        const attention = section(t('Needs you'), requestCards(true)); if (!approvals().length && !suggestions().length) attention.append(el('p', 'muted', t('Nothing needs you right now. New requests will appear here.'))); page.append(attention);
        const runningNow = section(t('Running now'));
        const runningHeading = el('div', 'section-heading', runningNow.firstElementChild, el('div', 'actions', ...(!s.data.control.paused ? [button(t('Pause new messages'), () => action(key, '/api/control/pause'))] : []), button(t('Stop everything'), () => { if (confirm(t('Stop everything? Running turns stop and queued messages are cancelled.'))) action(key, '/api/control/stop'); }, 'danger')));
        runningNow.append(runningHeading);
        const active = tasks().filter(task => ['running', 'queued'].includes(task.status));
        if (!active.length && !s.data.control.queue?.length) runningNow.append(el('p', 'muted', t('No turns are running. Send a message to a Marble to start.')));
        for (const task of active) runningNow.append(el('div', 'record', el('strong', '', (personForChat(task.conversationId)?.name || t('Marble')) + ' · ' + (task.status === 'queued' ? t('Queued') : t('Working'))), el('p', '', task.prompt), button(task.status === 'queued' ? t('Cancel') : t('Stop'), () => action(key, '/api/tasks/' + enc(task.id) + '/cancel'))));
        for (const item of s.data.control.queue || []) if (!active.some(task => task.id === item.id)) runningNow.append(el('div', 'record', el('strong', '', (person(item.marbleId)?.name || personForChat(item.conversationId)?.name || t('Marble')) + t(s.data.control.paused ? ' · Waiting for resume' : ' · Queued')), el('p', '', item.text), button(t('Cancel'), () => action(key, '/api/tasks/' + enc(item.id) + '/cancel')))); page.append(runningNow);
        const scheduled = section(t('Scheduled'));
        if (!s.data.routines?.routines?.length) scheduled.append(el('p', 'muted', t('No routines yet. Choose Add routine to schedule a message.')));
        for (const item of s.data.routines?.routines || []) {
            scheduled.append(el('div', 'record routine', el('h3', '', item.title || item.prompt), el('p', '', item.prompt), el('p', 'small muted', L.scheduleLabel(item.schedule)), el('p', 'small muted', item.enabled ? t('Next run: ') + L.relativeRun(item.nextRunAt) : t('Paused. Resume to schedule the next run.')), ...(item.missedRuns > 0 ? [el('p', 'small muted', t('Missed runs: ') + item.missedRuns)] : []), el('div', 'actions', ...[[t('Run now'), 'run'], [item.enabled ? t('Pause') : t('Resume'), item.enabled ? 'disable' : 'enable'], [t('Remove'), 'remove']].map(([label, suffix]) => button(label, () => action(key, '/api/routines/' + enc(item.id) + '/' + suffix))), button(item.catchUp ? t('Turn catch-up off') : t('Turn catch-up on'), () => action(key, '/api/routines/' + enc(item.id) + '/catch-up', { enabled: !item.catchUp })))));
        }
        const title = field(t('Title')); const prompt = field(t('Prompt'), '', 'textarea'); prompt.input.required = true;
        const schedule = selectField(t('Schedule'), [['every:1h', t('Every hour')], ['every:1d', t('Every 24 hours')], ['daily:09:00', t('Daily at 9:00')], ['custom', t('Custom')]], 'every:1h');
        const custom = field(t('Custom schedule'), 'every:2h'); custom.node.hidden = true;
        schedule.input.addEventListener('change', () => { custom.node.hidden = schedule.input.value !== 'custom'; custom.input.required = !custom.node.hidden; });
        const add = form('routine-add', [title, prompt, schedule, custom], () => action('routine-add', '/api/routines', { title: title.input.value, prompt: prompt.input.value, schedule: schedule.input.value === 'custom' ? custom.input.value : schedule.input.value }, () => { s.addRoutineOpen = false; }));
        add.dataset.form = 'routine'; add.querySelector('.primary').textContent = t('Add routine');
        const toggle = button(t('Add routine'), () => { s.addRoutineOpen = !s.addRoutineOpen; render(); if (s.addRoutineOpen) $('main').querySelector('[data-form="routine"] input')?.focus(); });
        toggle.setAttribute('aria-expanded', String(!!s.addRoutineOpen)); toggle.setAttribute('aria-controls', 'routine-editor'); scheduled.append(toggle);
        if (s.addRoutineOpen) {
            add.append(button(t('Cancel'), () => { s.addRoutineOpen = false; render(); }));
            scheduled.append(el('div', 'routine-editor', el('p', 'small muted', t('Custom: every:15m, every:2h, every:1d or daily:09:30. Times use the saved timezone; intervals are at least one minute.')), add)); scheduled.lastElementChild.id = 'routine-editor';
        }
        scheduled.append(button(t('Use this device’s timezone'), () => action(key, '/api/routines/offset', { offsetMinutes: -new Date().getTimezoneOffset() })));
        scheduled.append(el('p', 'small muted', t('Schedule UTC offset (minutes): ') + (s.data.routines.offsetMinutes || 0)));
        const quiet = quietFields(s.data.routines.quietHours || {});
        scheduled.append(disclosure(t('Quiet hours for scheduled messages'), form('routine-quiet', quiet.fields, () => action('routine-quiet', '/api/routines/quiet', quiet.read())), 'routine-quiet')); page.append(scheduled);
        const recent = section(t('Recent'));
        if (!L.recentTasks(tasks()).length) recent.append(el('p', 'muted', t('No finished turns yet. Your recent work will appear here.')));
        for (const task of L.recentTasks(tasks())) recent.append(el('div', 'record', el('strong', '', personForChat(task.conversationId)?.name || t('Marble')), el('p', '', task.prompt), el('p', 'small muted', task.status), el('div', 'actions', button(t('Open chat'), () => { const p = personForChat(task.conversationId); if (p?.isDefault) s.defaultChat = task.conversationId; location.hash = chatRoute(p?.id || 'marble'); }), ...(['failed', 'cancelled', 'interrupted'].includes(task.status) ? [retry(task)] : []))));
        page.append(recent); return page;
    }
    function quietFields(value) {
        const enabled = field(t('Quiet hours enabled'), value.enabled, 'checkbox');
        const start = field(t('Start time'), L.minuteTime(value.startMinute ?? 1320), 'time');
        const end = field(t('End time'), L.minuteTime(value.endMinute ?? 420), 'time');
        const offset = field(t('UTC offset (minutes)'), value.offsetMinutes ?? 0, 'number'); offset.input.min = -840; offset.input.max = 840;
        start.input.required = true; end.input.required = true; offset.input.required = true;
        return { fields: [enabled, start, end, offset], read: () => ({ enabled: enabled.input.checked, startMinute: L.timeMinute(start.input.value), endMinute: L.timeMinute(end.input.value), offsetMinutes: Number(offset.input.value) }) };
    }
    function settingsPage() {
        const key = 'settings'; const page = el('div', 'page settings-page', errorNode(key));
        const connectionValues = [[t('Host version'), s.health.version || t('Not available yet')], [t('Stream'), s.status.stream ? t('Open') : t('Reconnecting…')], [t('Engine'), s.health.runtime?.health === 'ok' ? t('Healthy') : s.health.runtime?.health ? t('Stopped') : t('Waiting for the host')], [t('Uptime'), Math.floor((s.data.presence?.uptimeSeconds || 0) / 60) + t(' minutes')]];
        const connectionSection = section(t('Connection'), el('dl', 'connection-values', ...connectionValues.map(([label, value]) => el('div', 'connection-row', el('dt', '', label), el('dd', '', value)))), el('p', 'error', s.health.runtime?.lastError || s.health.lastPersistError || ''));
        if (s.health.authRequired) connectionSection.append(button(t('Sign out'), async () => { await action(key, '/api/logout'); $('unlock').showModal(); s.stream?.close(); })); page.append(connectionSection);
        const posture = el('fieldset', 'posture-list', el('legend', 'visually-hidden', t('Permissions posture')));
        for (const [value, title, description] of [
            ['observe', t('Read only'), t('Read the workspace. No changes or approval prompts.')],
            ['workspace', t('Work in the workspace'), t('Ask before governed calls, unless a grant applies.')],
            ['unattended', t('Work unattended'), t('Approve governed calls without asking.')],
        ]) {
            const input = el('input'); input.type = 'radio'; input.name = 'posture'; input.value = value; input.id = 'posture-' + value;
            input.checked = value === (s.pendingProfile || s.data.control.profile); input.disabled = !!s.permissionSaving;
            input.addEventListener('change', async () => {
                if (!input.checked || s.permissionSaving) return;
                s.permissionSaving = true; s.pendingProfile = value; s.permissionNotice = ''; s.errors.delete('permissions'); render();
                const saved = await action('permissions', '/api/control/profile', { profile: value }, () => { s.data.control.profile = value; });
                s.permissionSaving = false; s.pendingProfile = null; s.permissionNotice = saved ? t('Saved') : ''; render();
                if (s.route.screen === 'settings' && document.activeElement === document.body) $('posture-' + (s.data.control.profile || value))?.focus({ preventScroll: true });
            });
            posture.append(el('label', 'posture-choice', input, el('span', '', el('strong', '', title), el('span', 'posture-description', description))));
        }
        const confirmation = el('p', 'small muted', s.permissionSaving ? t('Saving…') : s.permissionNotice || ''); confirmation.setAttribute('role', 'status');
        const permissions = section(t('Permissions'), posture, confirmation, errorNode('permissions'), el('p', 'small muted', t('Denied paths always apply. Engine-native tools run outside these approvals.')), el('h3', '', t('Standing grants')));
        if (!s.data.control.grants?.length) permissions.append(el('p', 'muted small', t('No standing grants. Choose “Always allow here” on a request to add one.')));
        for (const grant of s.data.control.grants || []) permissions.append(el('div', 'record', el('p', '', grant.tool + ' · ' + (grant.pathPrefix || t('Workspace'))), button(t('Revoke'), () => action(key, '/api/control/grants/' + enc(grant.id) + '/revoke'))));
        const denied = s.data.control.deniedPaths || []; permissions.append(el('h3', '', t('Denied paths')), el('p', 'muted small', denied.length ? denied.join(', ') : t('No denied paths are configured. Review permissions before starting work.')));
        const historyBody = el('div'); for (const decision of (s.data.control.ledger || []).slice(-30).reverse()) historyBody.append(el('p', 'small muted', [decision.at ? new Date(decision.at).toLocaleString() : '', decision.tool, decision.path, decision.decision, decision.reason].filter(Boolean).join(' · ')));
        if (!historyBody.childNodes.length) historyBody.append(el('p', 'muted small', t('No decisions recorded yet. Governed requests will appear here.')));
        permissions.append(disclosure(t('Decision history'), historyBody, 'decisions')); page.append(permissions);
        const model = field(t('Model for new chats'), s.data.model); model.input.placeholder = t('Engine default'); page.append(section(t('Model'), el('p', 'muted small', t('Applies to new chats. Leave empty to use the engine default.')), form('model', [model], () => action('model', '/api/model', { model: model.input.value }))));
        const notify = s.data.notifications || {}; const webhook = field(t('Webhook URL'), notify.webhookUrl, 'url'); const token = field(t('Webhook token'), '', 'password');
        token.input.placeholder = notify.webhookTokenSet ? t('Saved token kept if blank') : t('Optional bearer token');
        const clearToken = field(t('Remove saved webhook token'), false, 'checkbox');
        const eventLabels = { approval: t('Approval requests'), settled: t('Finished turns'), failed: t('Failures'), routine: t('Routine updates'), memory: t('Memory suggestions') };
        const eventFields = (notify.availableEvents || Object.keys(eventLabels)).map(event => ({ event, ...field(eventLabels[event] || event, (notify.events || []).includes(event), 'checkbox') }));
        const quiet = quietFields(notify.quietHours || {});
        const reAlert = field(t('Re-alert after (seconds; 0 turns it off)'), notify.reAlertSeconds ?? 300, 'number'); reAlert.input.min = 0; reAlert.input.max = 86400;
        const apns = notify.apns || {}; const keyFile = field(t('Apple push key file'), apns.keyFile); const team = field(t('Apple team ID'), apns.teamId); const keyId = field(t('Apple key ID'), apns.keyId); const topic = field(t('Apple topic'), apns.topic);
        const environment = selectField(t('Apple push environment'), [['sandbox', t('Sandbox')], ['production', t('Production')]], apns.environment || 'production');
        const retries = field(t('Delivery retries (0–5)'), notify.retries ?? 2, 'number'); retries.input.min = 0; retries.input.max = 5;
        const dedupe = field(t('Suppress duplicates for (seconds)'), notify.dedupeSeconds ?? 120, 'number'); dedupe.input.min = 0; dedupe.input.max = 86400;
        const advanced = disclosure(t('Apple push and delivery options'), el('div', '', ...[keyFile, team, keyId, topic, environment, retries, dedupe].map(f => f.node)), 'push-options');
        const notificationForm = form('notifications', [webhook, token, ...(notify.webhookTokenSet ? [clearToken] : []), ...eventFields, ...quiet.fields, reAlert, { node: advanced }], () => action('notifications', '/api/notifications', { webhookUrl: webhook.input.value, ...(clearToken.input.checked || token.input.value ? { webhookToken: clearToken.input.checked ? '' : token.input.value } : {}), events: eventFields.filter(f => f.input.checked).map(f => f.event), quietHours: quiet.read(), reAlertSeconds: Number(reAlert.input.value), retries: Number(retries.input.value), dedupeSeconds: Number(dedupe.input.value), apns: keyFile.input.value ? { keyFile: keyFile.input.value, teamId: team.input.value, keyId: keyId.input.value, topic: topic.input.value, environment: environment.input.value } : null }));
        notificationForm.dataset.form = 'notifications';
        const notificationSection = section(t('Notifications'), el('p', 'muted small', t('Save a webhook URL or configure Apple push to receive updates. Approvals and failures ignore quiet hours. Tests use saved settings.')), notificationForm);
        const reports = el('div', 'notification-reports'); reports.dataset.notificationReports = ''; reports.setAttribute('role', 'status');
        const showReports = () => { reports.replaceChildren(); if (!s.notificationTest) return; const lines = L.deliveryLines(s.notificationTest); for (const line of lines) reports.append(el('p', line.ok ? 'small' : 'small error', line.text)); if (!lines.length) reports.append(el('p', 'muted small', t('No channels are configured. Save a webhook URL or Apple push settings, then test again.'))); };
        showReports();
        notificationSection.append(button(t('Test notifications'), async () => {
            try { s.notificationTest = await api('/api/notifications/test', {}); render(); } catch (error) { showError('notifications', humanError(error)); }
        }));
        notificationSection.append(reports, button(t('Recent deliveries'), async () => { try { const report = await api('/api/notifications'); s.deliveries = report.recent || []; render(); } catch (error) { showError('notifications', humanError(error)); } }));
        const deliveryRows = () => s.deliveries?.length ? s.deliveries.slice(-20).reverse().map(item => el('p', 'small muted', [item.channel, item.ok ? t('Delivered') : t('Not delivered'), item.detail || item.error].filter(Boolean).join(' · '))) : [el('p', 'muted small', s.deliveries ? t('No deliveries recorded yet. Send a test notification to check your channels.') : t('No deliveries loaded. Choose Recent deliveries to check saved results.'))];
        const deliveries = el('div', '', ...deliveryRows()); notificationSection.append(deliveries, el('h3', '', t('Devices')));
        if (!s.data.devices?.length) notificationSection.append(el('p', 'muted small', t('No devices registered. Connect the iOS app to receive push notifications.')));
        for (const device of s.data.devices || []) notificationSection.append(el('div', 'record', el('p', 'small muted', device.platform + ' · ' + device.tokenPrefix), button(t('Remove device'), () => action(key, '/api/devices/' + enc(device.tokenPrefix) + '/remove')))); page.append(notificationSection);
        const voice = field(t('Read replies aloud'), s.data.voice?.enabled, 'checkbox');
        const rate = field(t('Speaking rate (0.1–1)'), s.data.voice?.rate ?? 0.5, 'number'); rate.input.min = 0.1; rate.input.max = 1; rate.input.step = 0.05;
        const voiceName = field(t('Voice name (optional)'), s.data.voice?.voice); page.append(section(t('Voice'), el('p', 'muted small', t('Speech stays on this device. Available voices depend on your browser.')), form('voice', [voice, rate, voiceName], () => action('voice', '/api/voice', { enabled: voice.input.checked, rate: Number(rate.input.value), voice: voiceName.input.value }))));
        const exportSection = section(t('Export'), el('p', 'muted small', t('Download a JSON copy of chats, memory, routines and settings.')), link(t('Export everything'), '/api/export', 'button'), button(t('Show workspace map'), async () => { try { s.workspaceMap = await api('/api/crates'); render(); } catch (error) { showError(key, humanError(error)); } }));
        if (s.workspaceMap) exportSection.append(disclosure(t('Workspace map'), prose(JSON.stringify(s.workspaceMap, null, 2)), 'workspace-map')); page.append(exportSection);
        page.append(section(t('Everyone'), button(t('Clear Everyone'), () => { if (confirm(t('Clear Everyone? Individual chats are kept.'))) action(key, '/api/everyone/clear'); }, 'danger'))); return page;
    }
    function replacePage(page) {
        const focused = document.activeElement; const start = focused.selectionStart; const end = focused.selectionEnd;
        for (const old of $('main').querySelectorAll('form[data-key]')) if (old.dataset.dirty || old.contains(focused)) {
            const replacement = [...page.querySelectorAll('form[data-key]')].find(form => form.dataset.key === old.dataset.key);
            if (replacement) replacement.replaceWith(old);
        }
        $('main').replaceChildren(page);
        if (page.contains(focused)) { focused.focus({ preventScroll: true }); if (typeof start === 'number') focused.setSelectionRange(start, end); }
    }
    function render() {
        renderRail();
        if (!$('topbar').querySelector('input:focus') && !$('topbar').querySelector('details[open]')) header();
        const info = s.route.screen === 'info'; const wideInfo = info && innerWidth >= 1200; const sheetInfo = info && innerWidth >= 900 && innerWidth < 1200;
        document.body.classList.toggle('info-layout', wideInfo); document.body.classList.toggle('info-sheet', sheetInfo); $('info-panel').hidden = !(wideInfo || sheetInfo);
        const editing = $('main').querySelector('form[data-dirty="true"]') || $('main').contains(document.activeElement) && document.activeElement.closest('form');
        if (!editing || ['activity', 'settings'].includes(s.route.screen)) {
            if (s.route.screen === 'everyone') renderTimeline(true);
            else if (s.route.screen === 'chat' || wideInfo || sheetInfo) { if (current()) renderTimeline(); else $('main').replaceChildren(el('div', 'empty', t('This Marble no longer exists'), link(t('Back to Marbles'), '#/marbles', 'button'))); }
            else if (s.route.screen === 'marbles') renderPeople();
            else if (info) $('main').replaceChildren(infoPage(current()));
            else if (s.route.screen === 'new') $('main').replaceChildren(newPage());
            else if (s.route.screen === 'activity') replacePage(activityPage());
            else if (s.route.screen === 'settings') replacePage(settingsPage());
            else $('main').replaceChildren(el('div', 'empty', t('Page not found')));
        }
        if ((wideInfo || sheetInfo) && ($('info-panel').dataset.marbleId !== s.route.id || !$('info-panel').contains(document.activeElement) && !$('info-panel').querySelector('form[data-dirty="true"]'))) {
            const close = link(icon('close'), chatRoute(s.route.id), 'button info-close'); close.setAttribute('aria-label', t('Close info'));
            $('info-panel').replaceChildren(...(wideInfo ? [close] : []), infoPage(current()));
            $('info-panel').dataset.marbleId = s.route.id;
        }
        composer();
    }
    async function refresh() {
        const generation = s.generation; const request = ++s.refreshId;
        try {
            const [data, people, shared] = await Promise.all([api('/api/state?lean=1'), api('/api/prism'), api('/api/everyone'), api('/healthz')]);
            if (generation !== s.generation || request !== s.refreshId) return;
            s.data = data; s.people = people.marbles || []; s.limit = people.limit; s.shapes = people.glyphs; s.shared = shared.entries || []; s.loaded = true;
            if (s.route.screen === 'info' && !s.tools) {
                try { s.tools = await api('/api/tools'); } catch (error) { s.toolsError = humanError(error); }
                if (generation !== s.generation || request !== s.refreshId) return;
            }
            if (s.pendingDefault) {
                const task = tasks().find(task => task.prompt === s.pendingDefault.text && task.conversationId && !s.pendingDefault.knownChats.has(task.conversationId));
                if (task) { s.defaultChat = task.conversationId; s.pendingDefault = null; }
            }
            if (s.defaultChat === undefined) s.defaultChat = defaultChats()[0]?.id || null;
            if (s.live.size > 128) s.live.delete(s.live.keys().next().value);
            if (['chat', 'info'].includes(s.route.screen) && conversation()) {
                const id = conversation();
                try {
                    const page = await api('/api/transcript?conversationId=' + enc(id));
                    if (generation !== s.generation || request !== s.refreshId || id !== conversation()) return;
                    const prior = new Set(s.entries.filter(entry => entry.kind === 'assistant').map(entry => entry.taskId));
                    if (s.entries.length && s.data.voice?.enabled) {
                        const newest = (page.entries || []).filter(entry => entry.kind === 'assistant' && !prior.has(entry.taskId)).at(-1);
                        if (newest) readAloud(newest.text);
                    }
                    s.entries = page.entries || []; s.missing = false; s.transcriptError = '';
                } catch (error) {
                    if (generation !== s.generation || request !== s.refreshId || id !== conversation()) return;
                    s.missing = error.status === 404; s.transcriptError = s.missing ? '' : humanError(error);
                    if (s.missing) { s.data.conversations = s.data.conversations.filter(chat => chat.id !== id); }
                }
            }
            render();
        } catch (error) { if (generation !== s.generation) return; if (error.status !== 401) { s.errors.set('conversation', humanError(error)); render(); } }
    }
    let refreshTimer;
    const refreshSoon = () => { clearTimeout(refreshTimer); refreshTimer = setTimeout(refresh, 90); };
    let frame;
    const drawSoon = () => { if (!frame) frame = requestAnimationFrame(() => { frame = 0; render(); }); };
    function onEvent(event) {
        let payload; try { payload = JSON.parse(event.data); } catch (_) { return; }
        if (event.type === 'hello') {
            if (Number.isSafeInteger(payload.cursor) && payload.cursor >= 0 && payload.cursor < s.sequence) {
                s.sequence = 0; s.live.clear(); s.approvalChats.clear(); refreshSoon();
            }
            return;
        }
        const sequence = Number(payload.sequence || event.lastEventId);
        if (sequence && sequence <= s.sequence) return;
        s.sequence = Math.max(s.sequence, sequence || 0);
        const kind = payload.kind || event.type; const data = payload.data || {}; const taskId = payload.taskId;
        if (['approval', 'approval_pending'].includes(kind) && data.requestId && payload.conversationId) s.approvalChats.set(data.requestId, payload.conversationId);
        if (kind === 'approval_resolved') s.approvalChats.delete(data.requestId);
        if (taskId && ['text', 'reasoning', 'tool', 'tool_start', 'tool_end', 'tool_progress', 'tool_cancel'].includes(kind)) {
            const live = s.live.get(taskId) || { conversationId: payload.conversationId, text: '', reasoning: '', tools: [] };
            if (kind === 'text' || kind === 'reasoning') live[kind] += L.messagePart(data, data.text || data.delta);
            else {
                const id = data.callId || data.call_id;
                const existing = live.tools.find(tool => id && tool.callId === id);
                const tool = { ...existing, ...data, text: String(data.text || existing?.text || '').slice(0, 4000), callId: id }; if (existing) Object.assign(existing, tool); else live.tools.push(tool);
            }
            s.live.set(taskId, live); drawSoon();
        } else if (kind === 'steer_rejected' || kind === 'error') {
            if (payload.conversationId === conversation()) { s.errors.set('conversation', humanError({ message: data.reason || data.message || t('Could not complete the request') })); drawSoon(); } refreshSoon();
        } else refreshSoon();
    }
    function connect() {
        s.stream?.close(); const stream = new EventSource('/api/events?since=' + s.sequence); s.stream = stream;
        stream.addEventListener('open', () => { s.status = L.reduceStatus(s.status, { type: 'open' }); renderStatus(); });
        stream.addEventListener('error', event => {
            if (event.data) return;
            s.status = L.reduceStatus(s.status, { type: 'close' }); renderStatus();
            if (stream.readyState === EventSource.CLOSED && !$('unlock').open) setTimeout(connect, 2000);
        });
        events.forEach(kind => stream.addEventListener(kind, onEvent));
    }
    window.addEventListener('hashchange', () => {
        s.route = L.route(location.hash); s.generation++; s.entries = []; s.missing = false; s.transcriptError = ''; s.errors.delete('conversation');
        s.addRoutineOpen = false;
        $('main').replaceChildren(); $('composer-area').replaceChildren(); render(); refresh(); window.scrollTo(0, 0); $('main').focus();
    });
    window.addEventListener('resize', () => { if (s.route.screen === 'info') render(); });
    $('unlock').addEventListener('cancel', event => event.preventDefault());
    $('unlock-form').addEventListener('submit', async event => {
        event.preventDefault(); $('unlock-error').textContent = '';
        try { await api('/api/login', { token: $('token').value }); $('token').value = ''; $('unlock').close(); connect(); await refresh(); }
        catch (error) { $('unlock-error').textContent = humanError(error); }
    });
    if (!location.hash) history.replaceState(null, '', '#/everyone');
    render(); refresh(); connect(); setInterval(refresh, 5000);
})();
