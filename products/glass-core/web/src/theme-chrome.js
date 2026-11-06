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

/** iOS appearance bit used only when theme mode is System. */
export function iosChromeIsLight(
  prefersLight = matchMedia("(prefers-color-scheme: light)").matches,
) {
  return prefersLight;
}

/** Status bar and theme-color follow iOS only while theme mode is System. */
export function chromeIsLight(
  mode,
  appLight,
  prefersLight = matchMedia("(prefers-color-scheme: light)").matches,
) {
  if (mode === "system") return iosChromeIsLight(prefersLight);
  return appLight;
}

export function resolveTheme(
  mode = storedThemeMode(),
  prefersLight = matchMedia("(prefers-color-scheme: light)").matches,
) {
  const appLight = workspaceIsLight(mode, prefersLight);
  const chromeLight = chromeIsLight(mode, appLight, prefersLight);
  return { mode, appLight, chromeLight, prefersLight };
}

export function sidebarThemeTarget(mode = storedThemeMode(), prefersLight) {
  if (mode === "dark") return "light";
  if (mode === "light") return "dark";
  if (prefersLight === undefined) {
    prefersLight = matchMedia("(prefers-color-scheme: light)").matches;
  }
  return resolveTheme("system", prefersLight).appLight ? "dark" : "light";
}

export function applyThemeChrome(root, appLight, chromeLight) {
  root.classList.toggle("light", appLight);
  root.classList.add("theme-ready");
  root.style.colorScheme = chromeLight ? "light" : "dark";
  root.style.backgroundColor = chromeLight ? "#f7f8f3" : "#141916";
  const bg = chromeLight ? "#f7f8f3" : "#141916";
  const doc = root.ownerDocument;
  for (const meta of doc.querySelectorAll('meta[name="theme-color"]')) meta.remove();
  const color = doc.createElement("meta");
  color.setAttribute("name", "theme-color");
  color.setAttribute("content", bg);
  doc.head.appendChild(color);
  let status = doc.querySelector('meta[name="apple-mobile-web-app-status-bar-style"]');
  if (status) status.remove();
  status = doc.createElement("meta");
  status.setAttribute("name", "apple-mobile-web-app-status-bar-style");
  status.setAttribute("content", chromeLight ? "default" : "black");
  doc.head.appendChild(status);
}

export function syncThemeFromStorage(root = document.documentElement) {
  const mode = storedThemeMode();
  const resolved = resolveTheme(mode);
  applyThemeChrome(root, resolved.appLight, resolved.chromeLight);
  return resolved;
}

/** Single entry used by sidebar, Appearance settings, and lifecycle refresh. */
export function chooseThemeMode(mode, root = document.documentElement) {
  if (!THEME_MODES.includes(mode)) mode = "system";
  persistThemeMode(mode);
  const resolved = resolveTheme(mode);
  applyThemeChrome(root, resolved.appLight, resolved.chromeLight);
  // WebKit often keeps the old status bar until metas are rewritten on a later frame.
  requestAnimationFrame(() =>
    applyThemeChrome(root, resolved.appLight, resolved.chromeLight),
  );
  document.dispatchEvent(
    new CustomEvent("glass-theme", { detail: { mode: resolved.mode } }),
  );
  return resolved;
}

let themeChromeLifecycleInstalled = false;

/** WebKit may ignore status-bar metadata until tags are rewritten after navigation restore. */
export function installThemeChromeLifecycle(root = document.documentElement) {
  if (themeChromeLifecycleInstalled) return;
  themeChromeLifecycleInstalled = true;
  const refresh = () => syncThemeFromStorage(root);
  window.addEventListener("pageshow", refresh);
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") refresh();
  });
  matchMedia("(prefers-color-scheme: light)").addEventListener("change", refresh);
}

/** @deprecated alias */
export const themeIsLight = workspaceIsLight;
export const applyThemeMode = chooseThemeMode;

export const installThemeChromeRefresh = installThemeChromeLifecycle;
