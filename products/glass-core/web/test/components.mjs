// Component rendering and fallback checks; no browser needed.
import assert from "node:assert/strict";
import * as ui from "../src/components.js";

assert.deepEqual(ui.client(), { name: "web", schema: 1, components: ui.TYPES });
assert.equal(ui.TYPES.length, 35);

const table = ui.parse('{"type":"table","v":1,"fallback":"| a |\\n| - |\\n| 1 |","data":{"title":"Hosts","columns":[{"key":"name","label":"Name"}],"rows":[{"name":"<script>"}],"filter":"name"}}');
const html = ui.render(table, "t1");
assert.match(html, /<table>/);
assert.match(html, /Hosts/);
assert.match(html, /&lt;script&gt;/, "data is escaped");
assert.match(html, /data-ui-action="filter"/);

assert.equal(ui.render({ type: "hologram", v: 1, data: {} }), null, "unknown types fall back");
assert.equal(ui.render({ type: "table", v: 2, data: {} }), null, "newer schemas fall back");
assert.equal(ui.fallback({ fallback: "Plain text" }), "Plain text");
assert.match(ui.fallback({ fallback: "Rates", example: true }), /^_Example data, not live\._/);
assert.match(ui.fallback(null, "{oops"), /```json\n\{oops/);
assert.equal(ui.parse("{not json"), null);
assert.equal(ui.parse('{"v":1}'), null, "a type is required");

const links = ui.render({ type: "news", v: 1, data: { items: [{ title: "Ok", url: "https://example.com/a", source: "Wire" }, { title: "Bad", url: "javascript:alert(1)" }] } });
assert.match(links, /href="https:\/\/example.com\/a"/);
assert.doesNotMatch(links, /javascript:/, "only http(s) links are kept");

const example = ui.render({ type: "weather", v: 1, example: true, data: { location: "Oslo", unit: "C", current: { temp: 4, condition: "Snow" } } });
assert.match(example, /Example data/);

const writing = ui.render({ type: "writing", v: 1, data: { kind: "email", to: "a@b.c", subject: "Hi", body: "Hello" } });
assert.match(writing, /data-ui-action="copy"/);
assert.match(writing, /mailto:/);
assert.doesNotMatch(writing, /data-ui-action="compose"/, "drafts are never sent from the component");

for (const kind of ["line", "bar", "scatter", "area", "pie"]) {
  const svg = ui.render({ type: "chart", v: 1, data: { kind, series: [{ name: "s", points: [{ x: "Mon", y: 2 }, { x: "Tue", y: 5 }] }] } });
  assert.match(svg, /<svg/, kind);
}
const form = ui.render({ type: "form", v: 1, data: { id: "trip", fields: [{ id: "who", label: "Name", kind: "text" }, { id: "size", label: "Size", kind: "radio", options: ["S", "M"] }] } });
assert.match(form, /<form class="ui-form"/);
assert.match(form, /type="radio"/);
assert.equal(ui.formAnswer("Trip", [["Name", "Ada"]]), "Trip:\n- Name: Ada");
console.log("components: ok");
