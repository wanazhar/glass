import katex from "katex";
import "katex/dist/katex.min.css";
export function math(source, display = false) {
  return katex.renderToString(source, {
    displayMode: display,
    throwOnError: false,
    trust: false,
    strict: "warn",
    maxExpand: 1000,
    maxSize: 20,
    output: "htmlAndMathml",
  });
}
