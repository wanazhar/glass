// Glass reply components (schema v1): a ```glass-ui fence holding one JSON
// object {type, v, fallback, example?, data}. The web UI draws every type and
// falls back to the stored Markdown for anything it can't read.
export const SCHEMA_VERSION = 1;
export const FENCE = "glass-ui";
export const TYPES = [
  "badges", "icon_list", "entities", "kpi", "buttons", "counter", "copy",
  "choices", "form", "table", "chart", "image", "gallery", "carousel", "map",
  "disclosure", "checklist", "progress", "timeline", "html", "product",
  "business", "reservation", "video", "sources", "file", "writing", "weather",
  "sports", "currency", "flights", "hotels", "jobs", "news", "flow",
];

/** What this client tells Core it can render, sent with every prompt. */
export const client = () => ({ name: "web", schema: SCHEMA_VERSION, components: TYPES });

const MAX_BYTES = 128 * 1024;

export function parse(source) {
  if (typeof source !== "string" || source.length > MAX_BYTES) return null;
  try {
    const value = JSON.parse(source.replace(/[\r\n]/g, "").trim());
    return value && typeof value === "object" && !Array.isArray(value) && typeof value.type === "string" ? value : null;
  } catch {
    return null;
  }
}

const esc = (value) =>
  String(value ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
const str = (value) => (value == null || typeof value === "object" ? "" : String(value));
const list = (value) => (Array.isArray(value) ? value.slice(0, 200) : []);
const num = (value) => (typeof value === "number" && Number.isFinite(value) ? value : Number.parseFloat(value));
export const safeUrl = (value, schemes = /^https?:$/) => {
  try {
    const url = new URL(str(value));
    return schemes.test(url.protocol) ? url.href : "";
  } catch {
    return "";
  }
};
const link = (href, body, cls = "") => {
  const url = safeUrl(href);
  return url ? `<a class="${cls}" href="${esc(url)}" rel="noopener noreferrer" target="_blank">${body}</a>` : body;
};
const tone = (value) => (["info", "success", "warning", "danger"].includes(value) ? value : "neutral");
const opt = (label, value) => (str(value) ? `<span><dt>${esc(label)}</dt><dd>${esc(value)}</dd></span>` : "");
const stars = (rating, reviews) => {
  const r = num(rating);
  if (!Number.isFinite(r)) return "";
  return `<span class="ui-rating" aria-label="Rated ${r.toFixed(1)} out of 5">★ ${esc(r.toFixed(1))}${reviews != null ? ` <span class="ui-dim">(${esc(reviews)})</span>` : ""}</span>`;
};
// Remote images are blocked by the page's content policy, so pictures show
// as links that open in a new tab.
const picture = (url, alt) => {
  const href = safeUrl(url);
  return href ? `<a class="ui-picture" href="${esc(href)}" rel="noopener noreferrer" target="_blank">▣ ${esc(alt || "Open image")}</a>` : "";
};

function card(item, extra = "") {
  const title = str(item.title || item.name);
  const lines = [item.subtitle, item.area, item.company, item.source, item.location, item.body, item.summary, item.snippet]
    .map(str).filter(Boolean);
  return `<article class="ui-card">${item.image ? picture(item.image, title) : ""}<strong>${link(item.url, esc(title))}</strong>${lines
    .map((line) => `<p>${esc(line)}</p>`).join("")}${extra}</article>`;
}

const PALETTE = ["var(--accent)", "var(--purple)", "var(--red)", "#9cc7e8", "#e8d49c", "#9ce8c5"];

function chart(d) {
  const kind = ["line", "bar", "scatter", "area", "pie"].includes(d.kind) ? d.kind : "line";
  const series = list(d.series).map((s) => ({ name: str(s.name), points: list(s.points).map((p) => ({ x: p.x, y: num(p.y) })).filter((p) => Number.isFinite(p.y)) }));
  const w = 320, h = 180, pad = 28;
  let body = "";
  if (kind === "pie") {
    const points = series[0]?.points || [];
    const total = points.reduce((sum, p) => sum + Math.max(0, p.y), 0) || 1;
    let angle = -Math.PI / 2;
    body = points.map((p, i) => {
      const a = (Math.max(0, p.y) / total) * Math.PI * 2;
      const large = a > Math.PI ? 1 : 0;
      const [x1, y1, x2, y2] = [Math.cos(angle), Math.sin(angle), Math.cos(angle + a), Math.sin(angle + a)].map((v, k) => (k % 2 ? 90 + v * 70 : 90 + v * 70));
      angle += a;
      return `<path d="M90 90 L${x1.toFixed(1)} ${y1.toFixed(1)} A70 70 0 ${large} 1 ${x2.toFixed(1)} ${y2.toFixed(1)} Z" fill="${PALETTE[i % PALETTE.length]}"><title>${esc(p.x)}: ${esc(p.y)}</title></path>`;
    }).join("");
    const legend = points.map((p, i) => `<li><i style="background:${PALETTE[i % PALETTE.length]}"></i>${esc(p.x)} <span class="ui-dim">${esc(p.y)}</span></li>`).join("");
    return `<div class="ui-chart-pie"><svg viewBox="0 0 180 180" role="img" aria-label="${esc(d.title || "Pie chart")}">${body}</svg><ul class="ui-legend">${legend}</ul></div>`;
  }
  const all = series.flatMap((s) => s.points);
  const labels = [...new Set(all.map((p) => str(p.x)))];
  const numericX = all.every((p) => Number.isFinite(num(p.x)));
  const xs = numericX ? all.map((p) => num(p.x)) : labels.map((_, i) => i);
  const [xmin, xmax] = [Math.min(...xs, 0), Math.max(...xs, 1)];
  const ys = all.map((p) => p.y);
  const [ymin, ymax] = [Math.min(0, ...ys), Math.max(1, ...ys)];
  const sx = (p) => pad + ((numericX ? num(p.x) : labels.indexOf(str(p.x))) - xmin) / (xmax - xmin || 1) * (w - pad * 2);
  const sy = (y) => h - pad - (y - ymin) / (ymax - ymin || 1) * (h - pad * 2);
  series.forEach((s, i) => {
    const color = PALETTE[i % PALETTE.length];
    if (kind === "bar") {
      const slot = (w - pad * 2) / Math.max(1, labels.length) / Math.max(1, series.length);
      body += s.points.map((p) => {
        const x = sx(p) - slot * series.length / 2 + slot * i;
        return `<rect x="${x.toFixed(1)}" y="${sy(Math.max(0, p.y)).toFixed(1)}" width="${Math.max(2, slot - 2).toFixed(1)}" height="${Math.abs(sy(p.y) - sy(0)).toFixed(1)}" fill="${color}"><title>${esc(s.name)} ${esc(p.x)}: ${esc(p.y)}</title></rect>`;
      }).join("");
    } else if (kind === "scatter") {
      body += s.points.map((p) => `<circle cx="${sx(p).toFixed(1)}" cy="${sy(p.y).toFixed(1)}" r="3.5" fill="${color}"><title>${esc(p.x)}, ${esc(p.y)}</title></circle>`).join("");
    } else {
      const path = s.points.map((p, k) => `${k ? "L" : "M"}${sx(p).toFixed(1)} ${sy(p.y).toFixed(1)}`).join(" ");
      if (kind === "area" && s.points.length)
        body += `<path d="${path} L${sx(s.points.at(-1)).toFixed(1)} ${sy(0).toFixed(1)} L${sx(s.points[0]).toFixed(1)} ${sy(0).toFixed(1)} Z" fill="${color}" fill-opacity="0.25"/>`;
      body += `<path d="${path}" fill="none" stroke="${color}" stroke-width="2"/>`;
    }
  });
  const axis = `<line x1="${pad}" y1="${h - pad}" x2="${w - pad}" y2="${h - pad}" class="ui-axis"/><line x1="${pad}" y1="${pad}" x2="${pad}" y2="${h - pad}" class="ui-axis"/><text x="${pad - 4}" y="${pad + 4}" text-anchor="end">${esc(+ymax.toFixed(2))}</text><text x="${pad - 4}" y="${h - pad}" text-anchor="end">${esc(+ymin.toFixed(2))}</text><text x="${pad}" y="${h - 8}">${esc(numericX ? xmin : labels[0] ?? "")}</text><text x="${w - pad}" y="${h - 8}" text-anchor="end">${esc(numericX ? xmax : labels.at(-1) ?? "")}</text>`;
  const legend = series.length > 1 ? `<ul class="ui-legend">${series.map((s, i) => `<li><i style="background:${PALETTE[i % PALETTE.length]}"></i>${esc(s.name)}</li>`).join("")}</ul>` : "";
  const caption = [d.x_label, d.y_label].map(str).filter(Boolean).join(" · ");
  return `<svg class="ui-chart" viewBox="0 0 ${w} ${h}" role="img" aria-label="${esc(d.title || kind + " chart")}">${axis}${body}</svg>${legend}${caption ? `<p class="ui-dim">${esc(caption)}</p>` : ""}`;
}

function field(f) {
  const id = esc(str(f.id) || str(f.label));
  const label = esc(f.label || f.id);
  const req = f.required ? " required" : "";
  const options = list(f.options).map((o) => (typeof o === "object" ? str(o.label || o.title || o.id) : str(o)));
  switch (f.kind) {
    case "select":
      return `<label>${label}<select data-field="${id}"${req}>${options.map((o) => `<option>${esc(o)}</option>`).join("")}</select></label>`;
    case "radio":
    case "checkbox":
      return `<fieldset data-field="${id}"><legend>${label}</legend>${options.map((o) => `<label class="ui-check"><input type="${f.kind}" data-option="${esc(o)}" data-group="${id}"> ${esc(o)}</label>`).join("")}</fieldset>`;
    case "slider":
      return `<label>${label}<input type="range" data-field="${id}" min="${esc(num(f.min) || 0)}" max="${esc(Number.isFinite(num(f.max)) ? num(f.max) : 100)}" step="${esc(num(f.step) || 1)}"></label>`;
    case "textarea":
      return `<label>${label}<textarea data-field="${id}" rows="3" placeholder="${esc(f.placeholder)}"${req}></textarea></label>`;
    default: {
      const type = ["email", "date"].includes(f.kind) ? f.kind : "text";
      return `<label>${label}<input type="${type}" data-field="${id}" placeholder="${esc(f.placeholder)}"${req}></label>`;
    }
  }
}

const RENDER = {
  badges: (d) => `<div class="ui-badges">${list(d.items).map((b) => `<span class="ui-badge ${tone(b.tone)}${b.style === "outlined" ? " outlined" : ""}">${esc(b.text)}</span>`).join("")}</div>`,
  icon_list: (d) => `<ul class="ui-rows">${list(d.items).map((i) => `<li><strong>${esc(i.title)}</strong>${i.caption ? `<span class="ui-dim">${esc(i.caption)}</span>` : ""}</li>`).join("")}</ul>`,
  entities: (d) => `<p class="ui-entities">${list(d.items).map((e) => `<span class="ui-entity">${link(e.url, esc(e.title))}${e.subtitle ? ` <span class="ui-dim">${esc(e.subtitle)}</span>` : ""}</span>`).join(" ")}</p>`,
  kpi: (d) => {
    const ranges = d.ranges && typeof d.ranges === "object" ? Object.entries(d.ranges).slice(0, 6) : [];
    const view = (v) => `<span class="ui-kpi-value">${esc(v.value)}</span>${v.delta != null ? `<span class="ui-delta ${v.trend === "down" ? "down" : v.trend === "up" ? "up" : ""}">${esc(v.delta)}</span>` : ""}`;
    return `<div class="ui-kpi"><div class="ui-kpi-line" data-ui-kpi>${view(d)}</div>${d.subtitle ? `<p class="ui-dim">${esc(d.subtitle)}</p>` : ""}${ranges.length ? `<div class="ui-segment" role="group" aria-label="Range">${ranges.map(([key, v]) => `<button type="button" data-ui-action="range" data-value="${esc(JSON.stringify(v))}">${esc(key)}</button>`).join("")}</div>` : ""}</div>`;
  },
  buttons: (d) => `<div class="ui-buttons">${list(d.items).map((b) => {
    const a = b.action || {};
    const cls = b.style === "primary" ? "primary" : b.style === "plain" ? "plain" : "";
    if (a.url && safeUrl(a.url)) return `<a class="ui-button ${cls}" href="${esc(safeUrl(a.url))}" rel="noopener noreferrer" target="_blank">${esc(b.label)}</a>`;
    if (a.copy != null) return `<button type="button" class="ui-button ${cls}" data-ui-action="copy" data-value="${esc(a.copy)}">${esc(b.label)}</button>`;
    return `<button type="button" class="ui-button ${cls}" data-ui-action="compose" data-value="${esc(a.send ?? b.label)}">${esc(b.label)}</button>`;
  }).join("")}</div>`,
  counter: (d) => `<div class="ui-counter" data-step="${esc(num(d.step) || 1)}" data-min="${esc(d.min ?? "")}" data-max="${esc(d.max ?? "")}"><span>${esc(d.label)}</span><button type="button" data-ui-action="count" data-value="-1" aria-label="Decrease">−</button><output data-ui-count>${esc(num(d.value) || 0)}</output><button type="button" data-ui-action="count" data-value="1" aria-label="Increase">+</button></div>`,
  copy: (d) => `<div class="ui-copy"><code>${esc(d.text)}</code><button type="button" data-ui-action="copy" data-value="${esc(d.text)}">${esc(d.label || "Copy")}</button></div>`,
  choices: (d) => `<div class="ui-choices" data-ui-group="${esc(d.id)}" data-multiple="${d.multiple ? "true" : ""}">${list(d.options).map((o) => `<button type="button" class="ui-choice" aria-pressed="false" data-ui-action="choose" data-value="${esc(o.title || o.id)}"><strong>${esc(o.title)}</strong>${o.subtitle ? `<span class="ui-dim">${esc(o.subtitle)}</span>` : ""}</button>`).join("")}<button type="button" class="ui-button primary" data-ui-action="choices">${esc(d.submit || "Use selection")}</button></div>`,
  form: (d) => `<form class="ui-form" data-ui-form="${esc(d.id)}">${list(d.fields).map(field).join("")}<button type="submit" class="ui-button primary">${esc(d.submit || "Add answers to message")}</button></form>`,
  table: (d) => {
    const columns = list(d.columns).map((c) => (typeof c === "object" ? { key: str(c.key), label: str(c.label || c.key) } : { key: str(c), label: str(c) }));
    const rows = list(d.rows);
    const filter = columns.find((c) => c.key === d.filter);
    const values = filter ? [...new Set(rows.map((r) => str(r[filter.key])))].slice(0, 30) : [];
    return `${filter ? `<label class="ui-filter">${esc(filter.label)} <select data-ui-action="filter"><option value="">All</option>${values.map((v) => `<option>${esc(v)}</option>`).join("")}</select></label>` : ""}<div class="table-wrap"><table><thead><tr>${columns.map((c) => `<th>${esc(c.label)}</th>`).join("")}</tr></thead><tbody>${rows.map((r) => `<tr${filter ? ` data-filter-value="${esc(str(r[filter.key]))}"` : ""}>${columns.map((c) => `<td>${esc(str(r?.[c.key]))}</td>`).join("")}</tr>`).join("")}</tbody></table></div>`;
  },
  chart,
  image: (d) => `${picture(d.url, d.alt)}${d.caption ? `<p class="ui-dim">${esc(d.caption)}</p>` : ""}`,
  gallery: (d) => `<div class="ui-strip">${list(d.images).map((i) => `<div>${picture(i.url, i.alt)}${i.caption ? `<p class="ui-dim">${esc(i.caption)}</p>` : ""}</div>`).join("")}</div>`,
  carousel: (d) => `<div class="ui-strip">${list(d.cards).map((c) => card(c)).join("")}</div>`,
  map: (d) => `<ul class="ui-rows">${list(d.pins).map((p) => `<li>${link(`https://www.openstreetmap.org/?mlat=${num(p.lat)}&mlon=${num(p.lon)}#map=${Math.round(num(d.zoom) || 14)}/${num(p.lat)}/${num(p.lon)}`, `⌖ ${esc(p.label)}`)}<span class="ui-dim">${esc(num(p.lat).toFixed(4))}, ${esc(num(p.lon).toFixed(4))}</span></li>`).join("")}</ul>`,
  disclosure: (d, _, markdown) => `<details class="ui-disclosure"><summary>${esc(d.title)}</summary><div class="ui-disclosure-body">${markdown(str(d.body))}</div></details>`,
  checklist: (d) => `<ul class="ui-checklist">${list(d.items).map((i) => `<li class="${i.done ? "done" : ""}"><span aria-hidden="true">${i.done ? "☑" : "☐"}</span> ${esc(i.text)}<span class="sr-only">${i.done ? ", done" : ", not done"}</span></li>`).join("")}</ul>`,
  progress: (d) => {
    const value = Math.min(1, Math.max(0, num(d.value) || 0));
    const percent = Math.round(value * 100);
    if (d.style === "ring")
      return `<div class="ui-ring" role="progressbar" aria-valuenow="${percent}" aria-valuemin="0" aria-valuemax="100" aria-label="${esc(d.label)}"><svg viewBox="0 0 36 36"><circle cx="18" cy="18" r="15.9" class="ui-track"/><circle cx="18" cy="18" r="15.9" class="ui-fill" stroke-dasharray="${percent} 100"/></svg><span>${percent}%</span><strong>${esc(d.label)}</strong></div>`;
    return `<div class="ui-progress${d.style === "gradient" ? " gradient" : ""}"><div class="ui-progress-head"><strong>${esc(d.label)}</strong><span>${percent}%</span></div><progress max="100" value="${percent}" aria-label="${esc(d.label)}"></progress>${d.caption ? `<p class="ui-dim">${esc(d.caption)}</p>` : ""}</div>`;
  },
  timeline: (d) => `<ol class="ui-timeline">${list(d.steps).map((s) => `<li class="${["done", "current"].includes(s.status) ? s.status : "pending"}"><strong>${esc(s.title)}</strong>${s.time ? ` <span class="ui-dim">${esc(s.time)}</span>` : ""}${s.detail ? `<p>${esc(s.detail)}</p>` : ""}</li>`).join("")}</ol>`,
  html: (d, id) => `<div class="ui-html"><code id="${id}" data-ui-html data-language="html" hidden>${esc(d.html)}</code><button type="button" data-html="${id}">Open interactive view</button></div>`,
  product: (d) => card({ ...d, subtitle: [d.price, d.merchant].map(str).filter(Boolean).join(" · ") }, `${stars(d.rating, d.reviews)}${list(d.badges).length ? `<div class="ui-badges">${list(d.badges).map((b) => `<span class="ui-badge neutral">${esc(b)}</span>`).join("")}</div>` : ""}`),
  business: (d) => card({ ...d, subtitle: [d.category, d.price_level].map(str).filter(Boolean).join(" · ") }, `${stars(d.rating, d.reviews)}<dl class="ui-facts">${opt("Address", d.address)}${opt("Hours", d.hours)}${opt("Phone", d.phone)}</dl>`),
  reservation: (d) => `<p><strong>${link(d.url, esc(d.venue))}</strong> <span class="ui-dim">${esc([d.date, d.party_size && `${d.party_size} people`].filter(Boolean).join(" · "))}</span></p><div class="ui-buttons">${list(d.slots).map((s) => (safeUrl(s.url) ? `<a class="ui-button" href="${esc(safeUrl(s.url))}" rel="noopener noreferrer" target="_blank">${esc(s.time)}</a>` : `<span class="ui-button" aria-disabled="true">${esc(s.time)}</span>`)).join("")}</div>`,
  video: (d) => card({ title: d.title, url: d.url, subtitle: str(d.duration) }),
  sources: (d) => `<ol class="ui-sources">${list(d.items).map((s) => `<li>${link(s.url, esc(s.title))}${s.site ? ` <span class="ui-dim">${esc(s.site)}</span>` : ""}${s.snippet ? `<p>${esc(s.snippet)}</p>` : ""}</li>`).join("")}</ol>`,
  file: (d) => `<div class="ui-card"><strong>${link(d.url, esc(d.name))}</strong><p class="ui-dim">${esc([d.kind, d.size, d.path].map(str).filter(Boolean).join(" · "))}</p>${d.excerpt ? `<pre>${esc(d.excerpt)}</pre>` : ""}</div>`,
  writing: (d) => `<div class="ui-writing"><dl class="ui-facts">${opt("To", d.to)}${opt("Subject", d.subject)}${opt("For", d.platform)}</dl><div class="ui-writing-body">${esc(d.body)}</div><div class="ui-buttons"><button type="button" class="ui-button" data-ui-action="copy" data-value="${esc([d.subject && `Subject: ${d.subject}`, d.body].filter(Boolean).join("\n\n"))}">Copy</button>${d.kind === "email" && safeUrl(`mailto:${d.to || ""}`, /^mailto:$/) ? `<a class="ui-button" href="mailto:${esc(encodeURIComponent(str(d.to)))}?subject=${esc(encodeURIComponent(str(d.subject)))}&amp;body=${esc(encodeURIComponent(str(d.body)))}">Open in Mail</a>` : ""}</div></div>`,
  weather: (d) => {
    const c = d.current || {};
    const unit = d.unit === "F" ? "°F" : "°C";
    return `<div class="ui-weather"><p><span class="ui-kpi-value">${esc(c.temp)}${unit}</span> ${esc(c.condition)}</p><p class="ui-dim">${esc([c.high != null && `High ${c.high}°`, c.low != null && `Low ${c.low}°`, c.humidity != null && `Humidity ${c.humidity}`, c.wind != null && `Wind ${c.wind}`].filter(Boolean).join(" · "))}</p>${list(d.forecast).length ? `<ul class="ui-forecast">${list(d.forecast).map((f) => `<li><strong>${esc(f.day)}</strong><span>${esc(f.condition)}</span><span>${esc(f.high)}° / ${esc(f.low)}°</span></li>`).join("")}</ul>` : ""}${d.as_of ? `<p class="ui-dim">As of ${esc(d.as_of)}</p>` : ""}</div>`;
  },
  sports: (d) => {
    const team = (t = {}) => `<div><strong>${esc(t.name)}</strong><span class="ui-kpi-value">${esc(t.score ?? "")}</span></div>`;
    return `<div class="ui-score">${team(d.home)}<span class="ui-dim">${esc(d.status)}</span>${team(d.away)}</div><p class="ui-dim">${esc([d.league, d.time, d.venue].map(str).filter(Boolean).join(" · "))}</p>`;
  },
  currency: (d) => `<p><span class="ui-kpi-value">${esc(d.converted)} ${esc(d.to)}</span></p><p class="ui-dim">${esc(d.amount)} ${esc(d.from)} at ${esc(d.rate)}${d.as_of ? ` · as of ${esc(d.as_of)}` : ""}</p>`,
  flights: (d) => `<ul class="ui-rows">${list(d.results).map((f) => `<li><strong>${link(f.url, esc(`${str(f.depart)} → ${str(f.arrive)}`))}</strong><span>${esc([f.airline, f.duration, f.stops != null && (f.stops ? `${f.stops} stop${f.stops > 1 ? "s" : ""}` : "Nonstop")].filter(Boolean).join(" · "))}</span>${f.price ? `<span class="ui-price">${esc(f.price)}</span>` : ""}</li>`).join("")}</ul>`,
  hotels: (d) => `<div class="ui-strip">${list(d.results).map((h) => card({ ...h, subtitle: [h.area, h.price].map(str).filter(Boolean).join(" · ") }, stars(h.rating))).join("")}</div>`,
  jobs: (d) => `<ul class="ui-rows">${list(d.results).map((j) => `<li><strong>${link(j.url, esc(j.title))}</strong><span>${esc([j.company, j.location].map(str).filter(Boolean).join(" · "))}</span><span class="ui-dim">${esc([j.salary, j.posted].map(str).filter(Boolean).join(" · "))}</span></li>`).join("")}</ul>`,
  news: (d) => `<ul class="ui-rows">${list(d.items).map((n) => `<li><strong>${link(n.url, esc(n.title))}</strong><span class="ui-dim">${esc([n.source, n.published].map(str).filter(Boolean).join(" · "))}</span>${n.summary ? `<p>${esc(n.summary)}</p>` : ""}</li>`).join("")}</ul>`,
  flow: (d) => `<ol class="ui-flow">${list(d.steps).map((s) => `<li><strong>${esc(s.title)}</strong>${s.detail ? `<p>${esc(s.detail)}</p>` : ""}</li>`).join("")}</ol>`,
};

const TITLES = { table: "Table", chart: "Chart", form: "Form", choices: "Choose", writing: "Draft", sources: "Sources", flights: "Flights", hotels: "Hotels", jobs: "Jobs", news: "News", weather: "Weather", sports: "Score", currency: "Currency", flow: "How it works", map: "Map", timeline: "Timeline", html: "Interactive view" };

/**
 * HTML for one component, or null when it can't be drawn (unknown type, a
 * newer schema, or bad JSON); callers then render the fallback Markdown.
 */
export function render(component, id, { markdown = esc } = {}) {
  const render = component && RENDER[component.type];
  if (!render || (component.v ?? 1) !== SCHEMA_VERSION) return null;
  const data = component.data && typeof component.data === "object" ? component.data : {};
  let body;
  try {
    body = render(data, id, markdown);
  } catch {
    return null;
  }
  const title = str(data.title);
  const head = title || TITLES[component.type]
    ? `<div class="block-head"><span>${esc(title || TITLES[component.type])}</span>${component.example ? '<span class="ui-example">Example data</span>' : ""}</div>`
    : component.example ? '<div class="block-head"><span></span><span class="ui-example">Example data</span></div>' : "";
  return `<figure class="ui-component ui-${esc(component.type)}" data-ui-type="${esc(component.type)}">${head}<div class="ui-body">${body}</div></figure>`;
}

/** The Markdown to show when a component can't be drawn. */
export function fallback(component, raw) {
  const text = str(component?.fallback).trim();
  if (text) return component.example && !text.startsWith("_Example") ? `_Example data, not live._\n\n${text}` : text;
  return "```json\n" + str(raw) + "\n```";
}

/** Text a form, choice group, or button puts into the message box. */
export function formAnswer(title, entries) {
  return [title ? `${title}:` : "", ...entries.map(([k, v]) => `- ${k}: ${v}`)].filter(Boolean).join("\n");
}
