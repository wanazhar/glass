import hljs from "highlight.js/lib/common";
export function highlight(source, language) {
  return hljs.getLanguage(language)
    ? hljs.highlight(source, { language, ignoreIllegals: true }).value
    : null;
}
