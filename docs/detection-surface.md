# Detection-Surface Transparency Report

**Version:** 1

**Chrome build:** Managed Chromium (see `glass install-chromium`)

**Methodology:** Stock Chrome launched via Glass's owned-process path
(`--remote-debugging-port`). No flags beyond Glass defaults. All measurements
taken through the page's own JavaScript context via `Runtime.evaluate`.

---

## Summary

Stock CDP-driven Chrome exposes several signals that distinguish it from an
organic browser session. This report documents them without mitigation claims.
Glass does not attempt to hide or alter these signals.

---

## 1. `navigator.webdriver`

| Property | Value |
|----------|-------|
| **Trigger** | Chrome sets this when launched with `--remote-debugging-port` |
| **Detected by** | `navigator.webdriver === true` |
| **Glass exposure** | Always `true` when Glass owns the browser |
| **Attach mode** | Also `true` — the CDP port is open regardless of launch method |

### Why It Exists

Chrome's WebDriver specification compliance: automation that uses the remote
debugging protocol must self-identify. This is intentional, not a leak.

### Detection Script

```javascript
JSON.stringify({
  webdriver: navigator.webdriver,
  userAgent: navigator.userAgent,
  platform: navigator.platform,
  languages: navigator.languages,
  hardwareConcurrency: navigator.hardwareConcurrency,
  deviceMemory: navigator.deviceMemory,
})
```

---

## 2. `Runtime.enable` Artifacts

| Signal | Visibility |
|--------|-----------|
| `Runtime.consoleAPICalled` events | Observable if the page hooks `console.*` |
| `Runtime.executionContextCreated` events | Not directly visible, but lifecycle differs from organic |
| Isolated worlds (`--worldName`) | Page scripts can detect `postMessage` or property tampering |

### Glass Impact

Glass creates isolated worlds named `glass` and `glass-observation` for its
own evaluation contexts. These worlds are deliberately **not** hidden. A page
can detect Glass's presence through side-channel timing if it instruments
`Runtime` lifecycle hooks.

---

## 3. Headless Mode Tells

Glass does **not** default to headless mode. `--headed` is implicit.

When a user optionally passes `--headless`:

| Signal | Value |
|--------|-------|
| `navigator.webdriver` | `true` |
| `window.chrome` object | Absent or partial |
| `navigator.plugins` | Empty or reduced |
| `screen.colorDepth` | May differ from headed |
| Font metric differences | Headless font rendering may produce slightly different bounding boxes |
| `requestAnimationFrame` | May not fire or fire at reduced rate |

### Glass Default

Glass launches **headed** Chrome. Headless mode is opt-in for CI. The headed
default reduces detection surface compared to headless-first tools.

---

## 4. CDP Port Exposure

| Signal | Value |
|--------|-------|
| **Port** | Default `9222`, configurable via `--port` |
| **HTTP endpoint** | `http://localhost:{port}/json` lists all debuggable targets |
| **WebSocket URL** | `ws://localhost:{port}/devtools/browser/{id}` |

The debugging port is bound to `localhost` only. Glass does not expose it on
external interfaces. An attacker with localhost access can enumerate targets;
this is consistent with the Chrome DevTools security model.

---

## 5. CDP Command Footprint

| Domain | Commands Used | Page-Visible Side Effects |
|--------|--------------|--------------------------|
| `Page` | `navigate`, `getFrameTree`, `createIsolatedWorld`, `captureScreenshot`, `handleJavaScriptDialog` | Isolated worlds detectable |
| `Runtime` | `evaluate`, `callFunctionOn`, `resolveNode`, `releaseObject`, `enable` | `Runtime.enable` instrumentation |
| `DOM` | `getDocument`, `getFlattenedDocument`, `querySelector`, `describeNode`, `resolveNode` | `DOM.documentUpdated` events |
| `Accessibility` | `getFullAXTree`, `getPartialAXTree` | Minimal — server-side computation |
| `Input` | `dispatchMouseEvent`, `dispatchKeyEvent` | Events dispatched as trusted; indistinguishable from user input |
| `Network` | `enable`, `setRequestInterception` (policy-gated) | Interception hooks observable |
| `Target` | `getTargets`, `createTarget`, `closeTarget`, `setAutoAttach`, `attachToTarget` | Target lifecycle; invisible to page |
| `Browser` | `getVersion`, `close`, `setDownloadBehavior` | Browser-level; invisible to page |

## 6. Non-Standard Headers

Glass does not inject non-standard HTTP headers by default. The `User-Agent`
header is whatever Chrome sends for the given platform and channel.

When `--policy polite` is active, Glass appends a declared Glass user-agent
suffix to `User-Agent`. This is opt-in and documented.

---

## 7. Observable Behavioural Differences

| Behaviour | Organic Chrome | Glass-Managed Chrome |
|-----------|---------------|---------------------|
| Mouse movement | Human (bézier curves with jitter) | `--interaction human` (default): bounded smooth paths |
| Typing speed | Variable per keystroke | `type` command sends events with configurable delay |
| Page lifecycle | User-paced | Script-paced; `wait` conditions gate progress |
| Tab management | User-initiated | CDP `Target.createTarget` / `Target.closeTarget` |
| Dialog handling | User clicks | CDP `Page.handleJavaScriptDialog` |

Page JavaScript **can** distinguish Glass from a human user through timing
analysis. `--interaction human` mimics human timing but is not undetectable.

---

## 8. What Glass Does NOT Expose

- Glass does **not** inject `<script>` tags into the page
- Glass does **not** modify `navigator.webdriver`
- Glass does **not** override `window.chrome`, `navigator.plugins`, or `navigator.mimeTypes`
- Glass does **not** spoof WebGL/Vulkan fingerprints
- Glass does **not** alter canvas or audio fingerprinting surfaces
- Glass policy engine does **not** silently redirect or rewrite requests

---

## Re-running This Report

To regenerate this report against a different Chrome build:

```sh
# Collect signals through evaluate
glass evaluate "JSON.stringify({
  webdriver: navigator.webdriver,
  plugins: Array.from(navigator.plugins).map(p => p.name),
  mimeTypes: Array.from(navigator.mimeTypes).map(m => m.type),
  languages: navigator.languages,
  hardwareConcurrency: navigator.hardwareConcurrency,
  deviceMemory: navigator.deviceMemory,
  platform: navigator.platform,
  userAgent: navigator.userAgent,
  vendor: navigator.vendor,
  cookieEnabled: navigator.cookieEnabled,
  doNotTrack: navigator.doNotTrack,
  onLine: navigator.onLine,
})"
```

Update this document when a Chrome upgrade or Glass release changes the
detection surface.

---

## Position

This report exists for transparency, not as a roadmap for evasion. Glass is
a local automation control plane, not a stealth browser. If your use case
requires avoiding bot detection, use the paths documented in the
[bot-protection runbook](bot-protection.md) instead.
