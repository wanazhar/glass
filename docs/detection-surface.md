# Detection-Surface Transparency Report

**Version:** 1

**Browser build:** Record the exact Chrome/Chromium version used for each run
(see `glass install-chromium`)

**Methodology:** Launch the selected Chrome/Chromium through Glass's owned
process path, record the full version and launch mode, and collect signals from
the page JavaScript context through explicit `evaluate`. The tables below are a
surface inventory; values that depend on Chrome version or headless mode must be
remeasured and retained with the run rather than treated as permanent facts.

---

## Summary

Stock CDP-driven Chrome exposes several signals that distinguish it from an
organic browser session. This report documents them without mitigation claims.
Glass does not attempt to hide or alter these signals.

---

## 1. `navigator.webdriver`

| Property | Value |
|----------|-------|
| **Possible trigger** | Chrome launch automation/debugging configuration; behavior is version-dependent |
| **Detected by** | `navigator.webdriver` in the page context |
| **Glass contract** | Glass does not clear or spoof the value |
| **Attach mode** | Measure the external browser; Glass did not choose its launch flags |

### Why It Exists

Do not infer the value from CDP connectivity alone. Chrome has changed the
conditions that set this signal across releases. The supported Glass claim is
that the runtime does not conceal or rewrite the browser's value.

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

Glass CLI sessions are headless by default. Pass `--headed` to request a
visible browser window. Record the selected mode because headless and headed
runs have different detection and rendering surfaces.

In headless mode, possible signals include:

| Signal | Value |
|--------|-------|
| `navigator.webdriver` | `true` |
| `window.chrome` object | Absent or partial |
| `navigator.plugins` | Empty or reduced |
| `screen.colorDepth` | May differ from headed |
| Font metric differences | Headless font rendering may produce slightly different bounding boxes |
| `requestAnimationFrame` | May not fire or fire at reduced rate |

The exact value of plugins, `window.chrome`, screen metrics, fonts, and frame
pacing depends on the browser build and host. Re-run the probe; do not publish
the illustrative possibilities above as observed values without evidence.

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
| `Runtime` | `evaluate`, `callFunctionOn`, `resolveNode`, `releaseObject`, `enable` | evaluation contexts and timing may be observable |
| `DOM` | `getDocument`, `getFlattenedDocument`, `querySelector`, `describeNode`, `resolveNode` | `DOM.documentUpdated` events |
| `Accessibility` | `getFullAXTree`, `getPartialAXTree` | Minimal — server-side computation |
| `Input` | `dispatchMouseEvent`, `dispatchKeyEvent` | Events dispatched as trusted; indistinguishable from user input |
| `Network` | enabled domains and policy-controlled network operations | timing and request behavior may be observable |
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

To regenerate this report against a different Chrome build, first keep the
session alive in the TUI or MCP lifecycle, navigate to a controlled local
fixture, and then collect the signals in that same session. The expression is:

```sh
# Collect signals through evaluate
evaluate "JSON.stringify({
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

Record `glass --version`, the browser version/path, headed/headless mode, OS,
launch versus attach ownership, and the raw bounded result. A standalone
one-shot `glass evaluate` without prior navigation measures a different page or
session and is not a reproduction of the intended fixture.

Update this document when a Chrome upgrade or Glass release changes the
detection surface.

---

## Position

This report exists for transparency, not as a roadmap for evasion. Glass is
a local automation control plane, not a stealth browser. If your use case
requires avoiding bot detection, use the paths documented in the
[bot-protection runbook](bot-protection.md) instead.
