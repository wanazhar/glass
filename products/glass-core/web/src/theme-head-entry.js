import {
  applyThemeChrome,
  storedThemeMode,
  themeIsLight,
} from "./theme-chrome.js";

applyThemeChrome(
  document.documentElement,
  themeIsLight(storedThemeMode()),
);
