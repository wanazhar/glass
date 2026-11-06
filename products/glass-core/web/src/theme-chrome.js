export function storedThemeMode() {
  try {
    const value = localStorage.getItem("glass:theme");
    if (value === "light" || value === "dark" || value === "system") return value;
  } catch {
    /* Browser storage may be disabled. */
  }
  return "system";
}

export const THEME_MODES = ["system", "light", "dark"];

export function nextThemeMode(mode = storedThemeMode()) {
  const index = THEME_MODES.indexOf(mode);
  return THEME_MODES[(index < 0 ? 0 : index + 1) % THEME_MODES.length];
}

export function themeModeLabel(mode) {
  if (mode === "light") return "Light workspace";
  if (mode === "dark") return "Dark workspace";
  return "System (matches iOS)";
}

export function persistThemeMode(mode) {
  if (!THEME_MODES.includes(mode)) return;
  try {
    localStorage.setItem("glass:theme", mode);
  } catch {
    /* Browser storage may be disabled. */
  }
}

/**
 * Workspace palette: System follows iOS; Light/Dark are explicit overrides.
 */
export function workspaceIsLight(
  mode,
  prefersLight = matchMedia("(prefers-color-scheme: light)").matches,
) {
  if (mode === "light") return true;
  if (mode === "dark") return false;
  return prefersLight;
}

/** Status bar and theme-color always follow iOS / Safari appearance. */
export function iosChromeIsLight(
  prefersLight = matchMedia("(prefers-color-scheme: light)").matches,
) {
  return prefersLight;
}

export function resolveTheme(
  mode = storedThemeMode(),
  prefersLight = matchMedia("(prefers-color-scheme: light)").matches,
) {
  const chromeLight = iosChromeIsLight(prefersLight);
  const appLight = workspaceIsLight(mode, prefersLight);
  return { mode, appLight, chromeLight, prefersLight };
}

export function applyThemeChrome(root, appLight, chromeLight) {
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
  const resolved = resolveTheme();
  applyThemeChrome(root, resolved.appLight, resolved.chromeLight);
  return resolved;
}

/** WebKit may ignore status-bar metadata until tags are rewritten after navigation restore. */
export function installThemeChromeLifecycle(root = document.documentElement) {
  const refresh = () => syncThemeFromStorage(root);
  refresh();
  window.addEventListener("pageshow", refresh);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") refresh();
  });
  matchMedia("(prefers-color-scheme: light)").addEventListener("change", refresh);
}

/** @deprecated alias */
export const themeIsLight = workspaceIsLight;

export const installThemeChromeRefresh = installThemeChromeLifecycle;
