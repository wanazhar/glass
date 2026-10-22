import mermaid from "mermaid";
let serial = 0;
let queue = Promise.resolve();
const cache = new Map();
const secure = [
  "secure",
  "securityLevel",
  "startOnLoad",
  "maxTextSize",
  "maxEdges",
  "htmlLabels",
  "themeCSS",
  "suppressErrorRendering",
];
function unique(svg) {
  const doc = new DOMParser().parseFromString(svg, "image/svg+xml");
  if (doc.querySelector("parsererror"))
    throw new Error("Diagram produced invalid SVG; source is preserved.");
  const prefix = `gc-copy-${++serial}-`;
  const ids = new Map();
  for (const node of doc.querySelectorAll("[id]")) {
    const old = node.id;
    ids.set(old, prefix + old);
    node.id = prefix + old;
  }
  for (const node of doc.querySelectorAll("*")) {
    for (const attribute of [...node.attributes]) {
      if (attribute.name === "id") continue;
      let value = attribute.value.replace(
        /url\(["']?#([^)'"\s]+)["']?\)/g,
        (match, id) => (ids.has(id) ? `url(#${ids.get(id)})` : match),
      );
      if (
        ["href", "xlink:href"].includes(attribute.name) &&
        value.startsWith("#") &&
        ids.has(value.slice(1))
      )
        value = "#" + ids.get(value.slice(1));
      if (["aria-labelledby", "aria-describedby"].includes(attribute.name))
        value = value
          .split(/\s+/)
          .map((id) => ids.get(id) || id)
          .join(" ");
      if (value !== attribute.value) attribute.value = value;
    }
    if (node.localName === "style") {
      // CSS selectors reference IDs; ordinary diagram text remains untouched.
      node.textContent = node.textContent.replace(/#([\w-]+)/g, (match, id) =>
        ids.has(id) ? "#" + ids.get(id) : match,
      );
    }
  }
  return new XMLSerializer().serializeToString(doc.documentElement);
}
export async function diagram(source) {
  source = source.replace(/^\uFEFF/, "").trim();
  if (source.length > 65536)
    throw new Error(
      "Diagram exceeds the 64 KiB preview limit. Open the source instead.",
    );
  const light = document.documentElement.classList.contains("light");
  const key = `${light}\n${source}`;
  let result = cache.get(key);
  if (!result) {
    result = queue.then(async () => {
      await document.fonts.ready;
      mermaid.initialize({
        startOnLoad: false,
        securityLevel: "strict",
        suppressErrorRendering: true,
        secure,
        htmlLabels: false,
        flowchart: { htmlLabels: false },
        theme: light ? "default" : "dark",
        maxTextSize: 65536,
        maxEdges: 500,
        fontFamily: "Glass Sans, sans-serif",
        themeVariables: {
          fontFamily: "Glass Sans, sans-serif",
          fontSize: "14px",
        },
      });
      const id = `glass-diagram-${++serial}`;
      try {
        return (await mermaid.render(id, source)).svg;
      } finally {
        document.getElementById(`d${id}`)?.remove();
      }
    });
    queue = result.catch(() => {});
    cache.set(key, result);
    if (cache.size > 32) cache.delete(cache.keys().next().value);
    result.catch(() => {
      if (cache.get(key) === result) cache.delete(key);
    });
  }
  return unique(await result);
}
