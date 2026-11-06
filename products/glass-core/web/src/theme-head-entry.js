import {
  applyThemeChrome,
  storedThemeMode,
  themeIsLight,
  iosChromeIsLight,
} from "./theme-chrome.js";

applyThemeChrome(
  document.documentElement,
  themeIsLight(storedThemeMode()),
  iosChromeIsLight(),
);
