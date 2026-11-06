// One quiet, user-controlled disclosure for adjacent reasoning and tool activity.
const storageKey = 'glass:activity-disclosures:v1';
const targets = new WeakMap();
let choices;
try { const source = sessionStorage.getItem(storageKey) || '[]'; choices = new Map((source.length < 65536 ? JSON.parse(source) : []).filter(item => Array.isArray(item) && typeof item[0] === 'string' && item[0].length <= 2048 && typeof item[1] === 'boolean').slice(-128)); }
catch { choices = new Map(); }
function identity(card, thread) {
  const call = card.querySelector('.tool-row[data-call-id]')?.dataset.callId;
  const record = card.dataset.recordIndex;
  const source = card.querySelector('.thought-excerpt')?.textContent.trim().slice(0, 64);
  let hash = 2166136261;
  for (const character of source || '') hash = Math.imul(hash ^ character.charCodeAt(0), 16777619);
  const thought = source ? (hash >>> 0).toString(36) : null;
  return call || thought || record ? `${thread}:${call || (thought ? `thought:${thought}` : `record:${record}`)}` : null;
}
function remember(card, thread) {
  const key = identity(card, thread);
  if (!key || key.length > 2048 || choices.get(key) === card.open) return;
  choices.delete(key); choices.set(key, card.open);
  while (choices.size > 128) choices.delete(choices.keys().next().value);
  try { sessionStorage.setItem(storageKey, JSON.stringify([...choices])); } catch {}
}
export function activityBody(target, thread) {
  let card = target.lastElementChild;
  if (!card?.classList.contains('activity-disclosure')) {
    card = document.createElement('details'); card.className = 'activity-disclosure';
    const summary = document.createElement('summary');
    const icon = document.createElement('span'); icon.className = 'activity-disclosure-icon'; icon.setAttribute('aria-hidden', 'true');
    const label = document.createElement('span'); label.className = 'activity-disclosure-label'; label.textContent = 'Activity';
    const count = document.createElement('span'); count.className = 'activity-disclosure-count';
    const chevron = document.createElement('span'); chevron.className = 'activity-disclosure-chevron'; chevron.textContent = '›'; chevron.setAttribute('aria-hidden', 'true');
    summary.append(icon, label, count, chevron);
    const body = document.createElement('div'); body.className = 'activity-disclosure-body';
    card.append(summary, body); target.append(card); targets.set(card, target);
    summary.addEventListener('click', () => { card.dataset.userChoice = 'true'; });
    card.addEventListener('toggle', () => {
      if (card.dataset.userChoice) remember(card, thread);
    });
  }
  return card.querySelector(':scope > .activity-disclosure-body');
}
export function refreshActivity(card, thread) {
  if (!card) return;
  const tools = [...card.querySelectorAll('.tool-row')];
  const thinking = [...card.querySelectorAll('.thought-label')].some(node => node.textContent === 'Thinking');
  const running = tools.some(node => node.dataset.state === 'running');
  const failed = tools.filter(node => node.dataset.state === 'failed').length;
  card.classList.toggle('is-active', running || thinking);
  card.querySelector('.activity-disclosure-label').textContent = thinking ? 'Thinking' : running ? 'Working' : 'Activity';
  card.querySelector('.activity-disclosure-count').textContent = [
    tools.length ? `${tools.length} ${tools.length === 1 ? 'tool' : 'tools'}` : '',
    card.querySelector('.thought-row') && !thinking ? 'Reasoning' : '',
    failed ? `${failed} failed` : '',
  ].filter(Boolean).join(' · ');
  const key = identity(card, thread);
  if (!card.dataset.userChoice && key && choices.has(key)) card.open = choices.get(key);
  if (card.dataset.userChoice) remember(card, thread);
}
export function refreshActivities(target, thread) {
  for (const card of target.querySelectorAll('.activity-disclosure')) refreshActivity(card, thread);
}

export function activityResult(card, result) {
  if (card.parentNode) card.after(result);
  else targets.get(card)?.append(result);
}
