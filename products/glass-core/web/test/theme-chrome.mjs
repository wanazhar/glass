import assert from "node:assert/strict";
import {
  resolveTheme,
  workspaceIsLight,
  iosChromeIsLight,
} from "../src/theme-chrome.js";

const light = true;
const dark = false;

assert.equal(workspaceIsLight("system", light), true);
assert.equal(workspaceIsLight("system", dark), false);
assert.equal(workspaceIsLight("light", dark), true);
assert.equal(workspaceIsLight("dark", light), false);

assert.equal(iosChromeIsLight(light), true);
assert.equal(iosChromeIsLight(dark), false);

let r = resolveTheme("system", light);
assert.equal(r.appLight, true);
assert.equal(r.chromeLight, true);

r = resolveTheme("system", dark);
assert.equal(r.appLight, false);
assert.equal(r.chromeLight, false);

r = resolveTheme("dark", light);
assert.equal(r.appLight, false);
assert.equal(r.chromeLight, true);

r = resolveTheme("light", dark);
assert.equal(r.appLight, true);
assert.equal(r.chromeLight, false);

console.log("PASS theme-chrome resolve rules");
