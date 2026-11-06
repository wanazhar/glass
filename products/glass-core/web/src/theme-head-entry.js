import { syncThemeFromStorage } from "./theme-chrome.js";

syncThemeFromStorage();

// Establish Home Screen surfaces before the launch screen can paint.
document.documentElement.classList.toggle('standalone',Boolean(navigator.standalone||matchMedia('(display-mode: standalone)').matches));
