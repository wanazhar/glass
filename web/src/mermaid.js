import mermaid from "mermaid";
mermaid.initialize({
  startOnLoad: false,
  htmlLabels: false,
  flowchart: { htmlLabels: false },
  securityLevel: "strict",
  theme: "dark",
  suppressErrorRendering: true,
  maxTextSize: 65536,
  maxEdges: 200,
  themeVariables: {
    primaryColor: "#22372f",
    primaryTextColor: "#e9ede6",
    primaryBorderColor: "#638b73",
    lineColor: "#84978c",
    fontFamily: "system-ui, sans-serif",
    fontSize: "14px",
  },
});
let serial = 0;
export async function diagram(source) {
  const { svg } = await mermaid.render("glass-diagram-" + ++serial, source);
  return svg;
}
