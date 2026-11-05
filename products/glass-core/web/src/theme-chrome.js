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

/** Keep iOS standalone status bar and theme-color aligned with the in-app theme. */
export function applyThemeChrome(root, light) {
  root.classList.toggle("light", light);
  root.classList.add("theme-ready");
  root.style.colorScheme = light ? "light" : "dark";
  const bg = light ? "#f7f8f3" : "#141916";
  const colors = root.ownerDocument.querySelectorAll('meta[name="theme-color"]');
  if (colors.length) {
    colors[0].setAttribute("content", bg);
    colors[0].removeAttribute("media");
    for (let i = 1; i < colors.length; i++) colors[i].remove();
  }
  const status = root.ownerDocument.querySelector(
    'meta[name="apple-mobile-web-app-status-bar-style"]',
  );
  if (status) status.setAttribute("content", light ? "default" : "black");
}
