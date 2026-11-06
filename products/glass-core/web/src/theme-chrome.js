export function storedThemeMode() {
  try {
    const value = localStorage.getItem("glass:theme");
    if (value === "light" || value === "dark" || value === "system") return value;
  } catch {
    /* Browser storage may be disabled. */
  }
  return "system";
}

export function themeIsLight(
  mode,
  prefersLight = matchMedia("(prefers-color-scheme: light)").matches,
) {
  return mode === "light" || (mode === "system" && prefersLight);
}

/** Status bar and theme-color follow Safari / iOS appearance, not Glass overrides. */
export function iosChromeIsLight(
  prefersLight = matchMedia("(prefers-color-scheme: light)").matches,
) {
  return prefersLight;
}

export function applyThemeChrome(
  root,
  appLight,
  chromeLight = iosChromeIsLight(),
) {
  root.classList.toggle("light", appLight);
  root.classList.add("theme-ready");
  root.style.colorScheme = chromeLight ? "light" : "dark";
  const bg = chromeLight ? "#f7f8f3" : "#141916";
  const doc = root.ownerDocument;
  let color = doc.querySelector('meta[name="theme-color"]');
  if (!color) {
    color = doc.createElement("meta");
    color.setAttribute("name", "theme-color");
    doc.head.appendChild(color);
  }
  color.setAttribute("content", bg);
  color.removeAttribute("media");
  for (const extra of doc.querySelectorAll('meta[name="theme-color"]')) {
    if (extra !== color) extra.remove();
  }
  const status = doc.querySelector(
    'meta[name="apple-mobile-web-app-status-bar-style"]',
  );
  if (status) status.setAttribute("content", chromeLight ? "default" : "black");
}

export function syncThemeFromStorage(root = document.documentElement) {
  applyThemeChrome(
    root,
    themeIsLight(storedThemeMode()),
    iosChromeIsLight(),
  );
}

/** iOS/Safari can drop status-bar metadata on bookmark restore until meta tags are touched again. */
export function installThemeChromeRefresh(root = document.documentElement) {
  const refresh = () => syncThemeFromStorage(root);
  refresh();
  window.addEventListener("pageshow", refresh);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") refresh();
  });
  window.addEventListener("focus", refresh);
  matchMedia("(prefers-color-scheme: light)").addEventListener("change", refresh);
}
